mod png;
mod publication;

use std::fmt::{self, Debug, Display, Formatter};

use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetRecord, AssetServer, AssetStates, AssetVersion,
    Assets, Handle, ImportArtifactPathError, ImportArtifactRecord, ImportDependencyDigest,
    ImportError, ImportProfile, ImportRequest, ImportSettingsHash, ImportedAssetType, Importer,
    ImporterDescriptor, ImporterDescriptorError, ImporterId, ImporterSelectionError,
    ImporterVersion, SourceExtension,
};
use nara_ecs::Resource;
use nara_fs::{DirectoryCapability, FileCapability, FsError, PathValidationError, RelativePath};
use nara_tasks::TaskCancellationToken;

use crate::budget::{
    ImageImportBudgetHost, ImageImportBudgetSnapshot, ImageImportCharge, ImageImportReservation,
};
use crate::limits::{
    ImageImportBudgetError, ImageImportLimits, ImageImportLimitsError, ImageImportMemoryPlan,
    file_admission_ceiling,
};
use crate::{ImageAsset, ImageColorSpace};

use png::{PngHeaderPreflight, check_encoded_limit};
pub use publication::ImageImportedAsset;
use publication::ImagePublicationAdmission;
pub(crate) use publication::ImagePublicationSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageImportStage {
    Admission,
    SourceOpen,
    SourceRead,
    Header,
    Metadata,
    Decode,
    Finalize,
    Publication,
}

impl ImageImportStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::SourceOpen => "source-open",
            Self::SourceRead => "source-read",
            Self::Header => "header",
            Self::Metadata => "metadata",
            Self::Decode => "decode",
            Self::Finalize => "finalize",
            Self::Publication => "publication",
        }
    }
}

impl Display for ImageImportStage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageUnsupportedFeature {
    Animation,
    Interlacing,
    EmbeddedMetadata,
    OutputColorModel,
}

impl ImageUnsupportedFeature {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Animation => "animation",
            Self::Interlacing => "interlacing",
            Self::EmbeddedMetadata => "embedded-metadata",
            Self::OutputColorModel => "output-color-model",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageSourceFailureKind {
    InvalidLogicalPath,
    NotFound,
    PermissionDenied,
    AllocationFailed,
    Unsupported,
    AuthorityRejected,
    Io,
}

impl ImageSourceFailureKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidLogicalPath => "invalid-logical-path",
            Self::NotFound => "not-found",
            Self::PermissionDenied => "permission-denied",
            Self::AllocationFailed => "allocation-failed",
            Self::Unsupported => "unsupported",
            Self::AuthorityRejected => "authority-rejected",
            Self::Io => "io",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImagePngFailureKind {
    Truncated,
    InvalidData,
    DecoderContract,
    AllocationFailed,
}

impl ImagePngFailureKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Truncated => "truncated",
            Self::InvalidData => "invalid-data",
            Self::DecoderContract => "decoder-contract",
            Self::AllocationFailed => "allocation-failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageImporterCreateError {
    Descriptor(ImporterDescriptorError),
    Limits(ImageImportLimitsError),
    BudgetLimitMismatch { configured: u64, host: u64 },
    BudgetOverlapLimitTooSmall { required: u64, host: u64 },
    BudgetAdmissionCeilingTooSmall { aggregate: u64, required: u64 },
}

impl Display for ImageImporterCreateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor(error) => Display::fmt(error, formatter),
            Self::Limits(error) => Display::fmt(error, formatter),
            Self::BudgetLimitMismatch { configured, host } => write!(
                formatter,
                "image aggregate limit {configured} does not match budget host limit {host}"
            ),
            Self::BudgetOverlapLimitTooSmall { required, host } => write!(
                formatter,
                "image publication overlap limit {required} exceeds budget host limit {host}"
            ),
            Self::BudgetAdmissionCeilingTooSmall {
                aggregate,
                required,
            } => write!(
                formatter,
                "image aggregate limit {aggregate} is below the shared-host file admission ceiling {required}"
            ),
        }
    }
}

impl std::error::Error for ImageImporterCreateError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageImportError {
    Selection(ImporterSelectionError),
    UnsupportedFormat {
        extension: SourceExtension,
    },
    ArtifactPath(ImportArtifactPathError),
    Budget {
        stage: ImageImportStage,
        error: ImageImportBudgetError,
    },
    Unsupported {
        stage: ImageImportStage,
        feature: ImageUnsupportedFeature,
    },
    Png {
        stage: ImageImportStage,
        kind: ImagePngFailureKind,
    },
    Source {
        stage: ImageImportStage,
        kind: ImageSourceFailureKind,
    },
    Publication(ImagePublicationFailureKind),
    Cancelled {
        stage: ImageImportStage,
    },
}

impl ImageImportError {
    #[must_use]
    pub const fn stage(&self) -> ImageImportStage {
        match self {
            Self::Selection(_) | Self::UnsupportedFormat { .. } | Self::ArtifactPath(_) => {
                ImageImportStage::Admission
            }
            Self::Budget { stage, .. } => *stage,
            Self::Unsupported { stage, .. }
            | Self::Png { stage, .. }
            | Self::Source { stage, .. }
            | Self::Cancelled { stage } => *stage,
            Self::Publication(_) => ImageImportStage::Publication,
        }
    }

    #[must_use]
    pub const fn stable_code(&self) -> &'static str {
        match self {
            Self::Selection(_) => "image.import-selection-failed",
            Self::UnsupportedFormat { .. } => "image.import-format-unsupported",
            Self::ArtifactPath(_) => "image.import-artifact-path-failed",
            Self::Budget { .. } => "image.import-budget-exceeded",
            Self::Unsupported { .. } => "image.import-feature-unsupported",
            Self::Png { .. } => "image.import-png-invalid",
            Self::Source { .. } => "image.import-source-failed",
            Self::Publication(_) => "image.import-publication-invalid",
            Self::Cancelled { .. } => "image.import-cancelled",
        }
    }

    #[must_use]
    pub const fn safe_summary(&self) -> &'static str {
        match self {
            Self::Selection(_) => "Image importer selection failed",
            Self::UnsupportedFormat { .. } => "Image format is unsupported",
            Self::ArtifactPath(_) => "Image artifact path creation failed",
            Self::Budget { .. } => "Image import budget was exceeded",
            Self::Unsupported { .. } => "PNG feature is unsupported",
            Self::Png { .. } => "PNG input is invalid",
            Self::Source { .. } => "Image source access failed",
            Self::Publication(_) => "Image publication admission is invalid",
            Self::Cancelled { .. } => "Image import was cancelled",
        }
    }

    fn budget(stage: ImageImportStage, error: ImageImportBudgetError) -> Self {
        Self::Budget { stage, error }
    }

    #[must_use]
    pub const fn budget_error(&self) -> Option<&ImageImportBudgetError> {
        match self {
            Self::Budget { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl Display for ImageImportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.safe_summary())
    }
}

impl std::error::Error for ImageImportError {}

pub struct ImageSourceDirectory {
    directory: DirectoryCapability,
}

impl ImageSourceDirectory {
    #[must_use]
    pub fn new(directory: DirectoryCapability) -> Self {
        Self { directory }
    }

    pub fn open(&self, record: &AssetRecord) -> Result<FileCapability, ImageImportError> {
        let path = RelativePath::new(record.path().as_str()).map_err(|error| {
            ImageImportError::Source {
                stage: ImageImportStage::SourceOpen,
                kind: map_path_error(error),
            }
        })?;
        self.directory
            .open_file(&path)
            .map_err(|error| map_fs_error(ImageImportStage::SourceOpen, error))
    }
}

impl Debug for ImageSourceDirectory {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageSourceDirectory")
            .field("directory_identity", &self.directory.identity())
            .finish_non_exhaustive()
    }
}

pub struct ImageBytesImportRequest {
    source: AssetRecord,
    source_bytes: Vec<u8>,
    dependency_digest: ImportDependencyDigest,
    settings_hash: ImportSettingsHash,
    profile: ImportProfile,
}

impl ImageBytesImportRequest {
    #[must_use]
    pub fn new(
        source: AssetRecord,
        source_bytes: impl Into<Vec<u8>>,
        dependency_digest: ImportDependencyDigest,
        settings_hash: ImportSettingsHash,
        profile: ImportProfile,
    ) -> Self {
        Self {
            source,
            source_bytes: source_bytes.into(),
            dependency_digest,
            settings_hash,
            profile,
        }
    }
}

impl Debug for ImageBytesImportRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageBytesImportRequest")
            .field("source", &self.source)
            .field("source_bytes", &self.source_bytes.len())
            .field("dependency_digest", &self.dependency_digest)
            .field("settings_hash", &self.settings_hash)
            .field("profile", &self.profile)
            .finish()
    }
}

pub struct ImageFileImportRequest {
    source: AssetRecord,
    file: FileCapability,
    dependency_digest: ImportDependencyDigest,
    settings_hash: ImportSettingsHash,
    profile: ImportProfile,
}

impl ImageFileImportRequest {
    #[must_use]
    pub fn new(
        source: AssetRecord,
        file: FileCapability,
        dependency_digest: ImportDependencyDigest,
        settings_hash: ImportSettingsHash,
        profile: ImportProfile,
    ) -> Self {
        Self {
            source,
            file,
            dependency_digest,
            settings_hash,
            profile,
        }
    }
}

impl Debug for ImageFileImportRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageFileImportRequest")
            .field("source", &self.source)
            .field("file", &self.file)
            .field("dependency_digest", &self.dependency_digest)
            .field("settings_hash", &self.settings_hash)
            .field("profile", &self.profile)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePublicationFailureKind {
    AlreadyLoaded,
    SlotChanged,
    TargetMismatch,
    StateChanged,
    ReloadValueMissing,
    ReloadValueChanged,
    UnknownAsset,
    VersionExhausted,
    Stale,
}

impl ImagePublicationFailureKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyLoaded => "already-loaded",
            Self::SlotChanged => "slot-changed",
            Self::TargetMismatch => "target-mismatch",
            Self::StateChanged => "state-changed",
            Self::ReloadValueMissing => "reload-value-missing",
            Self::ReloadValueChanged => "reload-value-changed",
            Self::UnknownAsset => "unknown-asset",
            Self::VersionExhausted => "version-exhausted",
            Self::Stale => "stale",
        }
    }

    #[must_use]
    pub const fn is_stale_conflict(self) -> bool {
        matches!(
            self,
            Self::AlreadyLoaded
                | Self::SlotChanged
                | Self::StateChanged
                | Self::ReloadValueMissing
                | Self::ReloadValueChanged
                | Self::Stale
        )
    }
}

impl Display for ImagePublicationFailureKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("image publication failed")
    }
}

impl std::error::Error for ImagePublicationFailureKind {}

struct BudgetedImageInput {
    bytes: Vec<u8>,
    reservation: ImageImportReservation,
}

impl BudgetedImageInput {
    fn new(bytes: Vec<u8>, reservation: ImageImportReservation) -> Self {
        Self { bytes, reservation }
    }

    fn parts_mut(&mut self) -> (&[u8], &mut ImageImportReservation) {
        (&self.bytes, &mut self.reservation)
    }

    fn release_encoded(self) -> ImageImportReservation {
        let Self { bytes, reservation } = self;
        drop(bytes);
        reservation
    }
}

/// An immutable image import preflight that is not bound to a runtime asset slot.
///
/// The token owns the exact source bytes used for preflight. Callers can reserve a wider host
/// budget from [`Self::memory_plan`] before consuming the token with [`Self::import`].
pub struct UnpublishedImageImport {
    importer: ImageImporter,
    request: ImageBytesImportRequest,
    preflight: PngHeaderPreflight,
    memory_plan: ImageImportMemoryPlan,
}

impl UnpublishedImageImport {
    #[must_use]
    pub const fn memory_plan(&self) -> ImageImportMemoryPlan {
        self.memory_plan
    }

    pub fn import(self) -> Result<ImageAsset, ImageImportError> {
        let Self {
            importer,
            request,
            preflight,
            memory_plan,
        } = self;
        let ImageBytesImportRequest {
            source,
            source_bytes,
            dependency_digest,
            settings_hash,
            profile,
        } = request;
        let charge = ImageImportCharge::admission(
            source_bytes.len(),
            0,
            importer.limits.max_in_flight_bytes(),
        )
        .map_err(|error| ImageImportError::budget(ImageImportStage::Admission, error))?;
        let reservation = importer
            .budget_host
            .reserve(charge)
            .map_err(|error| ImageImportError::budget(ImageImportStage::Admission, error))?;
        let mut input = BudgetedImageInput::new(source_bytes, reservation);
        let image = {
            let (bytes, reservation) = input.parts_mut();
            let request =
                ImportRequest::new(&source, bytes, dependency_digest, settings_hash, profile);
            let (image, observed_plan) = importer.decode_png_with_preflight(
                request,
                preflight,
                memory_plan,
                reservation,
                None,
            )?;
            debug_assert_eq!(observed_plan, memory_plan);
            image
        };
        drop(input.release_encoded());
        Ok(image)
    }
}

impl Debug for UnpublishedImageImport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UnpublishedImageImport")
            .field("request", &self.request)
            .field("memory_plan", &self.memory_plan)
            .finish_non_exhaustive()
    }
}

pub struct AdmittedImageImport {
    importer: ImageImporter,
    request: ImageFileImportRequest,
    publication: ImagePublicationAdmission,
    reservation: ImageImportReservation,
}

impl AdmittedImageImport {
    pub fn import(
        self,
        cancellation: TaskCancellationToken,
    ) -> Result<ImageImportedAsset, ImageImportError> {
        let Self {
            importer,
            request,
            publication,
            reservation,
        } = self;
        if cancellation.is_cancelled() {
            return Err(ImageImportError::Cancelled {
                stage: ImageImportStage::SourceRead,
            });
        }
        let encoded_limit = importer.limits.max_encoded_bytes().get();
        let bytes = request
            .file
            .read_to_end_bounded(encoded_limit as u64)
            .map_err(|error| map_fs_error(ImageImportStage::SourceRead, error))?;
        let mut input = BudgetedImageInput::new(bytes, reservation);
        if cancellation.is_cancelled() {
            return Err(ImageImportError::Cancelled {
                stage: ImageImportStage::Header,
            });
        }
        let (image, memory_plan) = {
            let (bytes, reservation) = input.parts_mut();
            let import_request = ImportRequest::new(
                &request.source,
                bytes,
                request.dependency_digest,
                request.settings_hash,
                request.profile,
            );
            importer.decode_png(
                import_request,
                bytes.len(),
                publication.overlap_bytes(),
                reservation,
                Some(&cancellation),
            )?
        };
        let reservation = input.release_encoded();
        ImageImportedAsset::new(image, memory_plan, publication, reservation)
            .retain_publication_charge()
    }
}

impl Debug for AdmittedImageImport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdmittedImageImport")
            .field("request", &self.request)
            .field("reservation", &self.reservation)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Resource)]
pub struct ImageImporter {
    descriptor: ImporterDescriptor,
    color_space: ImageColorSpace,
    limits: ImageImportLimits,
    budget_host: ImageImportBudgetHost,
}

impl Default for ImageImporter {
    fn default() -> Self {
        Self::with_limits(ImageImportLimits::default())
            .expect("built-in image importer configuration is valid")
    }
}

impl ImageImporter {
    pub fn new() -> Result<Self, ImageImporterCreateError> {
        Self::with_limits(ImageImportLimits::default())
    }

    pub fn with_limits(limits: ImageImportLimits) -> Result<Self, ImageImporterCreateError> {
        let budget_host =
            ImageImportBudgetHost::new(limits).map_err(ImageImporterCreateError::Limits)?;
        Self::with_budget_host(limits, budget_host)
    }

    pub fn with_budget_host(
        limits: ImageImportLimits,
        budget_host: ImageImportBudgetHost,
    ) -> Result<Self, ImageImporterCreateError> {
        let limits = limits
            .validate()
            .map_err(ImageImporterCreateError::Limits)?;
        if limits.max_in_flight_bytes() != budget_host.limit() {
            return Err(ImageImporterCreateError::BudgetLimitMismatch {
                configured: limits.max_in_flight_bytes().get() as u64,
                host: budget_host.limit().get() as u64,
            });
        }
        if limits.max_rgba_bytes().get() > budget_host.publication_overlap_limit().get() {
            return Err(ImageImporterCreateError::BudgetOverlapLimitTooSmall {
                required: limits.max_rgba_bytes().get() as u64,
                host: budget_host.publication_overlap_limit().get() as u64,
            });
        }
        let required_file_admission = file_admission_ceiling(
            limits.max_encoded_bytes().get(),
            budget_host.publication_overlap_limit().get(),
        );
        if limits.max_in_flight_bytes().get() < required_file_admission {
            return Err(ImageImporterCreateError::BudgetAdmissionCeilingTooSmall {
                aggregate: limits.max_in_flight_bytes().get() as u64,
                required: required_file_admission as u64,
            });
        }
        let descriptor = ImporterDescriptor::new(
            ImporterId::new("nara_image_png").expect("built-in importer id is valid"),
            ImporterVersion::new(2),
            [SourceExtension::new("png").expect("built-in source extension is valid")],
            ImportedAssetType::new("nara_image.image").expect("built-in output type is valid"),
            ArtifactFormatVersion::new(1),
        )
        .map_err(ImageImporterCreateError::Descriptor)?;
        Ok(Self {
            descriptor,
            color_space: ImageColorSpace::Srgb,
            limits,
            budget_host,
        })
    }

    #[must_use]
    pub fn with_color_space(mut self, color_space: ImageColorSpace) -> Self {
        self.color_space = color_space;
        self
    }

    #[must_use]
    pub const fn limits(&self) -> ImageImportLimits {
        self.limits
    }

    #[must_use]
    pub fn budget_snapshot(&self) -> ImageImportBudgetSnapshot {
        self.budget_host.snapshot()
    }

    #[must_use]
    pub fn budget_host(&self) -> ImageImportBudgetHost {
        self.budget_host.clone()
    }

    /// Preflights an immutable source buffer for an unpublished content snapshot.
    ///
    /// The returned token performs no decode allocation and assumes no last-good publication
    /// overlap. It binds the memory plan to the exact bytes later consumed by the decode.
    pub fn preflight_unpublished_import(
        &self,
        request: ImageBytesImportRequest,
    ) -> Result<UnpublishedImageImport, ImageImportError> {
        let (preflight, memory_plan) = self.preflight_png_memory_plan(
            &request.source,
            &request.source_bytes,
            request.source_bytes.len(),
            0,
        )?;
        Ok(UnpublishedImageImport {
            importer: self.clone(),
            request,
            preflight,
            memory_plan,
        })
    }

    pub fn admit_file(
        &self,
        request: ImageFileImportRequest,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<AdmittedImageImport, ImageImportError> {
        let publication = ImagePublicationSnapshot::capture(
            &request.source,
            handle,
            expected_version,
            images,
            states,
        );
        self.admit_file_with_snapshot(request, publication, asset_server, images, states)
    }

    pub(crate) fn admit_file_with_snapshot(
        &self,
        request: ImageFileImportRequest,
        publication: ImagePublicationSnapshot,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<AdmittedImageImport, ImageImportError> {
        validate_extension(request.source.path())?;
        let publication_overlap_bytes = self.budget_host.publication_overlap_limit().get();
        let publication = publication.admit(
            &request.source,
            asset_server,
            images,
            states,
            publication_overlap_bytes,
        )?;
        let charge = ImageImportCharge::admission(
            self.limits.max_encoded_bytes().get(),
            publication.overlap_bytes(),
            self.limits.max_in_flight_bytes(),
        )
        .map_err(|error| ImageImportError::budget(ImageImportStage::Admission, error))?;
        let reservation = self
            .budget_host
            .reserve(charge)
            .map_err(|error| ImageImportError::budget(ImageImportStage::Admission, error))?;
        Ok(AdmittedImageImport {
            importer: self.clone(),
            request,
            publication,
            reservation,
        })
    }

    pub fn import_image(
        &self,
        request: ImageBytesImportRequest,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
        asset_server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<ImageImportedAsset, ImageImportError> {
        let ImageBytesImportRequest {
            source,
            source_bytes,
            dependency_digest,
            settings_hash,
            profile,
        } = request;
        validate_extension(source.path())?;
        check_encoded_limit(self.limits, source_bytes.len())?;
        let publication_overlap_bytes = self.budget_host.publication_overlap_limit().get();
        let publication =
            ImagePublicationSnapshot::capture(&source, handle, expected_version, images, states)
                .admit(
                    &source,
                    asset_server,
                    images,
                    states,
                    publication_overlap_bytes,
                )?;
        let charge = ImageImportCharge::admission(
            source_bytes.len(),
            publication.overlap_bytes(),
            self.limits.max_in_flight_bytes(),
        )
        .map_err(|error| ImageImportError::budget(ImageImportStage::Admission, error))?;
        let reservation = self
            .budget_host
            .reserve(charge)
            .map_err(|error| ImageImportError::budget(ImageImportStage::Admission, error))?;
        let mut input = BudgetedImageInput::new(source_bytes, reservation);
        let (image, memory_plan) = {
            let (bytes, reservation) = input.parts_mut();
            let import_request =
                ImportRequest::new(&source, bytes, dependency_digest, settings_hash, profile);
            self.decode_png(
                import_request,
                bytes.len(),
                publication.overlap_bytes(),
                reservation,
                None,
            )?
        };
        let reservation = input.release_encoded();
        ImageImportedAsset::new(image, memory_plan, publication, reservation)
            .retain_publication_charge()
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
        validate_extension(request.source().path())
            .map_err(|_| ImportError::ImporterFailed("image import admission failed".to_owned()))?;
        self.import_record(&request)
            .map_err(|_| ImportError::ImporterFailed("image artifact creation failed".to_owned()))
    }
}

fn validate_extension(path: &nara_asset::AssetPath) -> Result<(), ImageImportError> {
    let extension = SourceExtension::from_asset_path(path).map_err(ImageImportError::Selection)?;
    let png_extension = SourceExtension::new("png").expect("built-in source extension is valid");
    if extension == png_extension {
        Ok(())
    } else {
        Err(ImageImportError::UnsupportedFormat { extension })
    }
}

fn map_path_error(_error: PathValidationError) -> ImageSourceFailureKind {
    ImageSourceFailureKind::InvalidLogicalPath
}

fn map_fs_error(stage: ImageImportStage, error: FsError) -> ImageImportError {
    match error {
        FsError::ByteLimitExceeded { limit } => ImageImportError::budget(
            stage,
            ImageImportBudgetError::per_image(
                crate::ImageImportLimitKind::EncodedBytes,
                Some(limit.saturating_add(1)),
                limit,
            ),
        ),
        FsError::Io { source, .. } => {
            let kind = match source.kind() {
                std::io::ErrorKind::NotFound => ImageSourceFailureKind::NotFound,
                std::io::ErrorKind::PermissionDenied => ImageSourceFailureKind::PermissionDenied,
                std::io::ErrorKind::OutOfMemory => ImageSourceFailureKind::AllocationFailed,
                _ => ImageSourceFailureKind::Io,
            };
            ImageImportError::Source { stage, kind }
        }
        FsError::Path(error) => ImageImportError::Source {
            stage,
            kind: map_path_error(error),
        },
        FsError::Unsupported { .. } => ImageImportError::Source {
            stage,
            kind: ImageSourceFailureKind::Unsupported,
        },
        FsError::Unproven { .. }
        | FsError::ReadOnlyCapability { .. }
        | FsError::NotDirectory
        | FsError::NotRegularFile
        | FsError::ReparsePoint { .. }
        | FsError::CrossVolume
        | FsError::MultipleLinks { .. }
        | FsError::IdentityUnavailable
        | FsError::CapabilitySessionExhausted
        | FsError::IdentityMismatch { .. }
        | FsError::TemporaryParentMismatch
        | FsError::AlreadyExists { .. }
        | FsError::TargetStateMismatch
        | FsError::LockContended
        | FsError::DigestMismatch { .. } => ImageImportError::Source {
            stage,
            kind: ImageSourceFailureKind::AuthorityRejected,
        },
    }
}
