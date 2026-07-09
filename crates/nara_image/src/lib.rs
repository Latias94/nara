//! Backend-neutral image assets and PNG-first importing.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_app::{App, CoreStage, Plugin, PluginError, TaskUpdateSet};
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvents, AssetLoadGenerations, AssetPath,
    AssetPlugin, AssetReloadRequest, AssetReloadRequestKind, AssetReloadRequests, AssetSourceKind,
    AssetSourceRoot, AssetStateError, AssetStates, Assets, Handle, ImportArtifactDigest,
    ImportArtifactPathError, ImportArtifactRecord, ImportError, ImportJobInput, ImportRequest,
    ImportedAsset, ImportedAssetType, Importer, ImporterDescriptor, ImporterDescriptorError,
    ImporterId, ImporterRegistry, ImporterRegistryError, ImporterSelectionError, ImporterVersion,
    LoadState, SourceExtension, SourceHash, StableAssetId, TypedImporter,
};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_render::{
    PreparedRenderResources, RenderPrepareApplyResult, RenderPrepareInvalidationReason,
    RenderPrepareInvalidations, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};
use nara_tasks::{TaskHandle, TaskPoolKind, TaskPools};

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
    pixels: Vec<u8>,
}

impl ImageAsset {
    #[must_use]
    pub fn new(
        source: ImageSourceMetadata,
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixels: Vec<u8>,
    ) -> Self {
        Self {
            source,
            extent,
            format,
            color_space,
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
        })
    }

    #[must_use]
    pub fn with_color_space(mut self, color_space: ImageColorSpace) -> Self {
        self.color_space = color_space;
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

impl TypedImporter<ImageAsset> for ImageImporter {
    fn descriptor(&self) -> &ImporterDescriptor {
        &self.descriptor
    }

    fn import_typed(
        &self,
        request: ImportRequest<'_>,
    ) -> Result<ImportedAsset<ImageAsset>, ImportError> {
        let imported = self
            .import_image(request)
            .map_err(|error| ImportError::ImporterFailed(error.to_string()))?;
        Ok(ImportedAsset::new(
            imported.artifact().clone(),
            imported.into_image(),
        ))
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
    pub removed: u32,
    pub skipped_missing_state: u32,
    pub skipped_not_loaded: u32,
    pub stale_results: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageReloadError {
    MissingSourceRoot { path: AssetPath },
    ReadSource { path: AssetPath, message: String },
    Import(ImportError),
}

impl Display for ImageReloadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSourceRoot { path } => {
                write!(
                    formatter,
                    "image reload for '{path}' requires AssetSourceRoot"
                )
            }
            Self::ReadSource { path, message } => {
                write!(formatter, "failed to read image source '{path}': {message}")
            }
            Self::Import(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ImageReloadError {}

#[derive(Debug, Default, Resource)]
pub struct ImageReloadStats {
    pub spawned: u32,
    pub applied: u32,
    pub failed: u32,
    pub stale: u32,
    pub cancelled: u32,
    pub removed: u32,
    pub pending: u32,
}

struct PendingImageImportJob {
    request: AssetReloadRequest,
    handle: TaskHandle<Result<ImportedAsset<ImageAsset>, ImageReloadError>>,
}

#[derive(Default, Resource)]
struct PendingImageJobs {
    imports: Vec<PendingImageImportJob>,
    removals: Vec<AssetReloadRequest>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImagePlugin;

impl Plugin for ImagePlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.image"),
            nara_app::PluginCategory::Asset,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(AssetPlugin)?;
        app.add_plugin_if_missing(ImagePreparePlugin)?;
        app.init_resource::<ImageReloadStats>();
        app.init_resource::<PendingImageJobs>();
        app.init_resource::<ImporterRegistry>();
        register_image_importer(app)?;
        app.add_systems(
            CoreStage::TaskUpdate,
            spawn_image_reload_jobs.in_set(TaskUpdateSet::SpawnAssetJobs),
        );
        app.add_systems(
            CoreStage::TaskUpdate,
            apply_image_reload_results.in_set(TaskUpdateSet::ApplyAssetResults),
        );
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ImagePreparePlugin;

impl Plugin for ImagePreparePlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.image.prepare"),
            nara_app::PluginCategory::Render,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Assets<ImageAsset>>();
        app.init_resource::<AssetStates>();
        app.init_resource::<PreparedRenderResources<PreparedImageResource>>();
        app.init_resource::<RenderPrepareInvalidations>();
        app.init_resource::<ImagePrepareStats>();
        app.add_systems(CoreStage::Prepare, prepare_images);
        Ok(())
    }
}

fn register_image_importer(app: &mut App) -> Result<(), PluginError> {
    let importer = ImageImporter::default();
    let id = Importer::descriptor(&importer).id().clone();
    let mut registry = app.world_mut().resource_mut::<ImporterRegistry>();
    if registry.importer(&id).is_some() {
        return Ok(());
    }
    registry
        .register(importer)
        .map_err(|error| image_plugin_setup_error("register image importer", error))
}

fn image_plugin_setup_error(context: &'static str, error: ImporterRegistryError) -> PluginError {
    PluginError::SetupFailed {
        plugin: nara_app::PluginId::new("nara.image"),
        message: format!("{context}: {error}"),
    }
}

fn spawn_image_reload_jobs(
    mut requests: ResMut<AssetReloadRequests>,
    source_root: Option<Res<AssetSourceRoot>>,
    task_pools: Res<TaskPools>,
    mut pending: ResMut<PendingImageJobs>,
    mut stats: ResMut<ImageReloadStats>,
) {
    for request in requests.drain_for_source_kind(&AssetSourceKind::Image) {
        match request.request_kind() {
            AssetReloadRequestKind::Remove => pending.removals.push(request),
            AssetReloadRequestKind::LoadOrReload => {
                let source_path = source_root
                    .as_ref()
                    .map(|source_root| source_root.source_path(request.path()));
                let record = request.record();
                let logical_path = request.path().clone();
                let handle = task_pools.spawn(TaskPoolKind::Io, move |_| {
                    let source_path =
                        source_path.ok_or_else(|| ImageReloadError::MissingSourceRoot {
                            path: logical_path.clone(),
                        })?;
                    let bytes = std::fs::read(&source_path).map_err(|error| {
                        ImageReloadError::ReadSource {
                            path: logical_path,
                            message: error.to_string(),
                        }
                    })?;
                    let input = ImportJobInput::new(
                        record,
                        bytes,
                        nara_asset::ImportDependencyDigest::empty(),
                        nara_asset::ImportSettingsHash::default(),
                        nara_asset::ImportProfile::default(),
                    );
                    ImageImporter::default()
                        .import_job(&input)
                        .map_err(ImageReloadError::Import)
                });
                pending
                    .imports
                    .push(PendingImageImportJob { request, handle });
                stats.spawned = stats.spawned.saturating_add(1);
            }
        }
    }
    stats.pending = pending.imports.len().min(u32::MAX as usize) as u32;
}

fn apply_image_reload_results(
    mut pending: ResMut<PendingImageJobs>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
    generations: Res<AssetLoadGenerations>,
    mut stats: ResMut<ImageReloadStats>,
) {
    let mut removals = std::mem::take(&mut pending.removals);
    removals.sort_by_key(AssetReloadRequest::id);
    for request in removals {
        if !generations.is_current(request.asset_id(), request.generation()) {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }
        let handle = Handle::<ImageAsset>::new(request.asset_id());
        match images.remove_with_state(handle, &mut states, &mut events) {
            Ok(_) => stats.removed = stats.removed.saturating_add(1),
            Err(_) => stats.failed = stats.failed.saturating_add(1),
        }
    }

    let mut unfinished = Vec::new();
    let mut finished = Vec::new();
    for mut job in pending.imports.drain(..) {
        if let Some(result) = job.handle.try_take() {
            finished.push((job.request, result));
        } else {
            unfinished.push(job);
        }
    }
    pending.imports = unfinished;
    finished.sort_by_key(|(request, _)| request.id());

    for (request, result) in finished {
        if result.is_cancelled() {
            stats.cancelled = stats.cancelled.saturating_add(1);
            continue;
        }
        if !generations.is_current(request.asset_id(), request.generation()) {
            stats.stale = stats.stale.saturating_add(1);
            continue;
        }

        match result.into_value() {
            Ok(imported) => apply_imported_image(
                request,
                imported,
                &mut images,
                &mut states,
                &mut events,
                &mut stats,
            ),
            Err(error) => record_image_reload_failure(
                request,
                error,
                &mut images,
                &mut states,
                &mut events,
                &mut stats,
            ),
        }
    }

    stats.pending = pending.imports.len().min(u32::MAX as usize) as u32;
}

fn apply_imported_image(
    request: AssetReloadRequest,
    imported: ImportedAsset<ImageAsset>,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    stats: &mut ImageReloadStats,
) {
    let handle = Handle::<ImageAsset>::new(request.asset_id());
    let source_hash = imported.value().source().source_hash();
    let artifact_hash = imported.artifact().key().digest();
    let result = if images.get(handle).is_some() {
        images.commit_reload(
            handle,
            request.expected_version(),
            imported.into_value(),
            states,
            events,
            Some(source_hash),
            Some(artifact_hash),
        )
    } else {
        if let Err(error) = states.ensure_version(handle.id(), request.expected_version()) {
            match error {
                AssetStateError::StaleReload { .. } => {
                    stats.stale = stats.stale.saturating_add(1);
                }
                _ => {
                    stats.failed = stats.failed.saturating_add(1);
                }
            }
            return;
        }
        images.commit_loaded(
            handle,
            imported.into_value(),
            states,
            events,
            Some(source_hash),
            Some(artifact_hash),
        )
    };

    match result {
        Ok(_) => stats.applied = stats.applied.saturating_add(1),
        Err(AssetStateError::StaleReload { .. }) => stats.stale = stats.stale.saturating_add(1),
        Err(_) => stats.failed = stats.failed.saturating_add(1),
    }
}

fn record_image_reload_failure(
    request: AssetReloadRequest,
    error: ImageReloadError,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
    stats: &mut ImageReloadStats,
) {
    let handle = Handle::<ImageAsset>::new(request.asset_id());
    if let Err(error) = states.ensure_version(handle.id(), request.expected_version()) {
        match error {
            AssetStateError::StaleReload { .. } => stats.stale = stats.stale.saturating_add(1),
            _ => stats.failed = stats.failed.saturating_add(1),
        }
        return;
    }
    let result = if images.get(handle).is_some() {
        images.record_reload_failure(handle, states, events, error.to_string())
    } else {
        images.record_load_failure(handle, states, events, error.to_string())
    };

    match result {
        Ok(_) => stats.failed = stats.failed.saturating_add(1),
        Err(_) => stats.stale = stats.stale.saturating_add(1),
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
    let removed_keys = prepared_images
        .keys()
        .filter(|key| key.kind() == RenderResourceKind::IMAGE_2D)
        .filter(|key| {
            let handle = Handle::<ImageAsset>::new(key.asset_id());
            images.get(handle).is_none()
                || states
                    .state(key.asset_id())
                    .is_none_or(|state| state.load_state() == &LoadState::Removed)
        })
        .collect::<Vec<_>>();
    for key in removed_keys {
        if prepared_images
            .remove(
                key,
                &mut invalidations,
                RenderPrepareInvalidationReason::AssetRemoved,
            )
            .is_some()
        {
            stats.removed += 1;
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use nara_app::App;
    use nara_asset::{
        AssetEventKind, AssetEvents, AssetLoadGeneration, AssetRecord, AssetReloadRequest,
        AssetReloadRequestKind, AssetReloadRequests, AssetServer, AssetSourceChangeKind,
        AssetSourceChanges, AssetSourceKind, AssetSourceRoot, AssetStates, AssetVersion, Assets,
        ImportDependencyDigest, ImportJobInput, ImportProfile, ImportSettingsHash,
        ImporterRegistry,
    };
    use nara_tasks::TaskPools;

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
    fn typed_importer_uses_owned_job_input() {
        let record = image_record("textures/player.png");
        let input = ImportJobInput::new(
            record,
            rgba_png(1, 1, &[0, 0, 255, 255]),
            ImportDependencyDigest::empty(),
            ImportSettingsHash::default(),
            ImportProfile::default(),
        );

        let imported = ImageImporter::default().import_job(&input).unwrap();

        assert_eq!(imported.value().extent(), ImageExtent::new(1, 1));
        assert_eq!(
            imported.artifact().key().output_asset_type().as_str(),
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
        let (mut app, handle) = app_with_loaded_image(ImageImporter::default());

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
    fn prepare_system_invalidates_when_image_descriptor_changes() {
        let (mut app, handle) = app_with_loaded_image(ImageImporter::default());
        app.update();
        app.world_mut()
            .resource_mut::<RenderPrepareInvalidations>()
            .drain();
        let old_snapshot = app
            .world()
            .resource::<PreparedRenderResources<PreparedImageResource>>()
            .get(image_resource_key(handle))
            .unwrap()
            .snapshot();

        let changed_importer = ImageImporter::default().with_color_space(ImageColorSpace::Linear);
        let changed = changed_importer
            .import_image(request(
                &image_record("textures/player.png"),
                &rgba_png(1, 1, &[0, 0, 255, 255]),
            ))
            .unwrap();
        let mut images = app
            .world_mut()
            .remove_resource::<Assets<ImageAsset>>()
            .unwrap();
        let mut states = app.world_mut().remove_resource::<AssetStates>().unwrap();
        let expected_version = states.version(handle.id()).unwrap();
        let source_hash = changed.image().source().source_hash();
        let artifact_hash = changed.artifact().key().digest();
        images
            .commit_reload(
                handle,
                expected_version,
                changed.into_image(),
                &mut states,
                &mut AssetEvents::default(),
                Some(source_hash),
                Some(artifact_hash),
            )
            .unwrap();
        app.world_mut().insert_resource(images);
        app.world_mut().insert_resource(states);

        app.update();

        let prepared = app
            .world()
            .resource::<PreparedRenderResources<PreparedImageResource>>();
        let new_snapshot = prepared.get(image_resource_key(handle)).unwrap().snapshot();
        assert_ne!(old_snapshot, new_snapshot);
        assert_eq!(
            new_snapshot.asset_version().raw(),
            expected_version.raw() + 1
        );
        assert_eq!(app.world().resource::<ImagePrepareStats>().prepared, 1);
        assert!(
            app.world()
                .resource::<RenderPrepareInvalidations>()
                .iter()
                .any(
                    |invalidation| invalidation.key() == image_resource_key(handle)
                        && invalidation.reason()
                            == RenderPrepareInvalidationReason::DescriptorChanged
                )
        );
    }

    #[test]
    fn prepare_system_removes_prepared_resources_for_removed_images() {
        let (mut app, handle) = app_with_loaded_image(ImageImporter::default());
        app.update();
        app.world_mut()
            .resource_mut::<RenderPrepareInvalidations>()
            .drain();
        assert!(
            app.world()
                .resource::<PreparedRenderResources<PreparedImageResource>>()
                .get_ready(image_resource_key(handle))
                .is_some()
        );

        let mut images = app
            .world_mut()
            .remove_resource::<Assets<ImageAsset>>()
            .unwrap();
        let mut states = app.world_mut().remove_resource::<AssetStates>().unwrap();
        images
            .remove_with_state(handle, &mut states, &mut AssetEvents::default())
            .unwrap();
        app.world_mut().insert_resource(images);
        app.world_mut().insert_resource(states);

        app.update();

        assert!(
            app.world()
                .resource::<PreparedRenderResources<PreparedImageResource>>()
                .get_ready(image_resource_key(handle))
                .is_none()
        );
        assert_eq!(app.world().resource::<ImagePrepareStats>().removed, 1);
        assert!(
            app.world()
                .resource::<RenderPrepareInvalidations>()
                .iter()
                .any(
                    |invalidation| invalidation.key() == image_resource_key(handle)
                        && invalidation.reason() == RenderPrepareInvalidationReason::AssetRemoved
                )
        );
    }

    #[test]
    fn image_plugin_loads_and_reloads_image_through_task_update() {
        let temp_root = unique_temp_root();
        let texture_path = temp_root.join("textures").join("player.png");
        fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
        fs::write(&texture_path, rgba_png(1, 1, &[255, 0, 0, 255])).unwrap();
        let record = image_record("textures/player.png");
        let mut app = app_with_image_plugin(&temp_root, record.clone());
        let handle = app
            .world_mut()
            .resource_mut::<AssetServer>()
            .reserve_record::<ImageAsset>(&record)
            .unwrap();

        app.world_mut()
            .resource_mut::<AssetSourceChanges>()
            .modified(record.path().clone());
        app.update();

        let first_hash = app
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .source_hash();
        assert_eq!(app.world().resource::<ImageReloadStats>().applied, 1);
        assert!(
            app.world()
                .resource::<PreparedRenderResources<PreparedImageResource>>()
                .get_ready(image_resource_key(handle))
                .is_some()
        );

        app.world_mut()
            .resource_mut::<RenderPrepareInvalidations>()
            .drain();
        fs::write(&texture_path, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
        app.world_mut()
            .resource_mut::<AssetSourceChanges>()
            .modified(record.path().clone());
        app.update();

        let image = app
            .world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap();
        assert_ne!(image.source().source_hash(), first_hash);
        assert_eq!(app.world().resource::<ImageReloadStats>().applied, 2);
        assert!(
            app.world()
                .resource::<RenderPrepareInvalidations>()
                .iter()
                .any(
                    |invalidation| invalidation.key() == image_resource_key(handle)
                        && invalidation.reason()
                            == RenderPrepareInvalidationReason::DescriptorChanged
                )
        );

        remove_temp_root(&temp_root);
    }

    #[test]
    fn image_plugin_records_first_load_failure_without_asset_value() {
        let temp_root = unique_temp_root();
        let texture_path = temp_root.join("textures").join("player.png");
        fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
        fs::write(&texture_path, b"not a png").unwrap();
        let record = image_record("textures/player.png");
        let mut app = app_with_image_plugin(&temp_root, record.clone());
        let handle = app
            .world_mut()
            .resource_mut::<AssetServer>()
            .reserve_record::<ImageAsset>(&record)
            .unwrap();

        app.world_mut()
            .resource_mut::<AssetSourceChanges>()
            .modified(record.path().clone());
        app.update();

        assert!(
            app.world()
                .resource::<Assets<ImageAsset>>()
                .get(handle)
                .is_none()
        );
        assert!(matches!(
            app.world()
                .resource::<AssetStates>()
                .state(handle.id())
                .unwrap()
                .load_state(),
            LoadState::Failed { .. }
        ));
        assert!(
            app.world()
                .resource::<AssetEvents>()
                .iter()
                .any(|event| event.kind() == AssetEventKind::LoadFailed)
        );

        remove_temp_root(&temp_root);
    }

    #[test]
    fn stale_first_load_success_cannot_recreate_removed_image() {
        let record = image_record("textures/player.png");
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::<ImageAsset>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let expected_version = states.set_loading(handle.id());
        images
            .remove_with_state(handle, &mut states, &mut events)
            .unwrap();
        events.drain();
        let imported = ImageImporter::default()
            .import_job(&ImportJobInput::new(
                record.clone(),
                rgba_png(1, 1, &[0, 0, 255, 255]),
                ImportDependencyDigest::empty(),
                ImportSettingsHash::default(),
                ImportProfile::default(),
            ))
            .unwrap();
        let request = reload_request(&record, handle, expected_version);
        let mut stats = ImageReloadStats::default();

        apply_imported_image(
            request,
            imported,
            &mut images,
            &mut states,
            &mut events,
            &mut stats,
        );

        assert_eq!(stats.stale, 1);
        assert!(images.get(handle).is_none());
        assert_eq!(
            states.state(handle.id()).unwrap().load_state(),
            &LoadState::Removed
        );
        assert!(events.drain().is_empty());
    }

    #[test]
    fn stale_first_load_failure_cannot_overwrite_newer_state() {
        let record = image_record("textures/player.png");
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::<ImageAsset>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let expected_version = states.set_loading(handle.id());
        images
            .remove_with_state(handle, &mut states, &mut events)
            .unwrap();
        events.drain();
        let request = reload_request(&record, handle, expected_version);
        let mut stats = ImageReloadStats::default();

        record_image_reload_failure(
            request,
            ImageReloadError::MissingSourceRoot {
                path: record.path().clone(),
            },
            &mut images,
            &mut states,
            &mut events,
            &mut stats,
        );

        assert_eq!(stats.stale, 1);
        assert_eq!(
            states.state(handle.id()).unwrap().load_state(),
            &LoadState::Removed
        );
        assert!(events.drain().is_empty());
    }

    #[test]
    fn image_plugin_removes_runtime_and_prepared_image() {
        let temp_root = unique_temp_root();
        let texture_path = temp_root.join("textures").join("player.png");
        fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
        fs::write(&texture_path, rgba_png(1, 1, &[255, 0, 0, 255])).unwrap();
        let record = image_record("textures/player.png");
        let mut app = app_with_image_plugin(&temp_root, record.clone());
        let handle = app
            .world_mut()
            .resource_mut::<AssetServer>()
            .reserve_record::<ImageAsset>(&record)
            .unwrap();
        app.world_mut()
            .resource_mut::<AssetSourceChanges>()
            .modified(record.path().clone());
        app.update();
        app.world_mut()
            .resource_mut::<RenderPrepareInvalidations>()
            .drain();

        app.world_mut()
            .resource_mut::<AssetSourceChanges>()
            .removed(record.path().clone());
        app.update();

        assert!(
            app.world()
                .resource::<Assets<ImageAsset>>()
                .get(handle)
                .is_none()
        );
        assert_eq!(
            app.world()
                .resource::<AssetStates>()
                .state(handle.id())
                .unwrap()
                .load_state(),
            &LoadState::Removed
        );
        assert!(
            app.world()
                .resource::<PreparedRenderResources<PreparedImageResource>>()
                .get_ready(image_resource_key(handle))
                .is_none()
        );
        assert!(
            app.world()
                .resource::<RenderPrepareInvalidations>()
                .iter()
                .any(
                    |invalidation| invalidation.key() == image_resource_key(handle)
                        && invalidation.reason() == RenderPrepareInvalidationReason::AssetRemoved
                )
        );

        remove_temp_root(&temp_root);
    }

    #[test]
    fn descriptor_hash_changes_when_content_descriptor_changes() {
        let record = image_record("textures/player.png");
        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
        let image = ImageImporter::default()
            .import_image(request(&record, &bytes))
            .unwrap()
            .into_image();
        let mut changed = image.clone();
        changed.color_space = ImageColorSpace::Linear;

        assert_ne!(
            image_descriptor_hash(&image),
            image_descriptor_hash(&changed)
        );
    }

    fn app_with_loaded_image(importer: ImageImporter) -> (App, Handle<ImageAsset>) {
        let record = image_record("textures/player.png");
        let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
        let imported = importer.import_image(request(&record, &bytes)).unwrap();
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let mut images = Assets::<ImageAsset>::default();
        let mut states = AssetStates::default();
        let source_hash = imported.image().source().source_hash();
        let artifact_hash = imported.artifact().key().digest();
        images
            .commit_loaded(
                handle,
                imported.into_image(),
                &mut states,
                &mut AssetEvents::default(),
                Some(source_hash),
                Some(artifact_hash),
            )
            .unwrap();
        let mut app = App::new();
        app.add_plugin(nara_render::RenderPlugin).unwrap();
        app.add_plugin(ImagePreparePlugin).unwrap();
        app.world_mut().insert_resource(images);
        app.world_mut().insert_resource(states);
        (app, handle)
    }

    fn app_with_image_plugin(asset_root: &Path, record: AssetRecord) -> App {
        let mut app = App::new();
        app.insert_resource(TaskPools::deterministic());
        app.insert_resource(AssetSourceRoot::new(asset_root));
        app.add_plugin(nara_render::RenderPlugin).unwrap();
        app.add_plugin(ImagePlugin).unwrap();
        app.world_mut()
            .resource_mut::<nara_asset::ProjectAssetDatabase>()
            .insert(record)
            .unwrap();
        app
    }

    fn reload_request(
        record: &AssetRecord,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
    ) -> AssetReloadRequest {
        let mut requests = AssetReloadRequests::new();
        requests.push_resolved(
            handle.id(),
            record,
            AssetReloadRequestKind::LoadOrReload,
            AssetSourceChangeKind::Modified,
            expected_version,
            AssetLoadGeneration::ZERO,
            Vec::new(),
        );
        requests
            .drain_for_source_kind(&AssetSourceKind::Image)
            .into_iter()
            .next()
            .unwrap()
    }

    fn unique_temp_root() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nara_image_test_{}_{}", std::process::id(), stamp))
    }

    fn remove_temp_root(path: &Path) {
        if let Err(error) = fs::remove_dir_all(path) {
            panic!(
                "failed to remove temp test directory {}: {error}",
                path.display()
            );
        }
    }
}
