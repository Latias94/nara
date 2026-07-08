use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
};

use crate::{
    ArtifactFormatVersion, ArtifactLabel, AssetPath, AssetRecord, ImportArtifactKey,
    ImportArtifactPathError, ImportArtifactRecord, ImportDependencyDigest, ImportLabelError,
    ImportLabelKind, ImportProfile, ImportSettingsHash, ImportedAssetType, ImporterId,
    ImporterVersion, SourceHash, artifact::validate_import_label,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceExtension(String);

impl SourceExtension {
    pub fn new(extension: impl AsRef<str>) -> Result<Self, ImportLabelError> {
        let extension = extension
            .as_ref()
            .strip_prefix('.')
            .unwrap_or(extension.as_ref());
        validate_import_label(ImportLabelKind::SourceExtension, extension).map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SourceExtension {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SourceExtension {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SourceExtension {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImporterDescriptor {
    id: ImporterId,
    version: ImporterVersion,
    supported_extensions: Vec<SourceExtension>,
    output_asset_type: ImportedAssetType,
    artifact_format_version: ArtifactFormatVersion,
}

impl ImporterDescriptor {
    pub fn new(
        id: ImporterId,
        version: ImporterVersion,
        supported_extensions: impl IntoIterator<Item = SourceExtension>,
        output_asset_type: ImportedAssetType,
        artifact_format_version: ArtifactFormatVersion,
    ) -> Result<Self, ImporterDescriptorError> {
        let supported_extensions = supported_extensions
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if supported_extensions.is_empty() {
            return Err(ImporterDescriptorError::NoSupportedExtensions { id });
        }

        Ok(Self {
            id,
            version,
            supported_extensions,
            output_asset_type,
            artifact_format_version,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &ImporterId {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> ImporterVersion {
        self.version
    }

    #[must_use]
    pub fn supported_extensions(&self) -> &[SourceExtension] {
        &self.supported_extensions
    }

    #[must_use]
    pub const fn output_asset_type(&self) -> &ImportedAssetType {
        &self.output_asset_type
    }

    #[must_use]
    pub const fn artifact_format_version(&self) -> ArtifactFormatVersion {
        self.artifact_format_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImporterDescriptorError {
    NoSupportedExtensions { id: ImporterId },
}

impl Display for ImporterDescriptorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSupportedExtensions { id } => {
                write!(
                    formatter,
                    "importer '{id}' has no supported source extensions"
                )
            }
        }
    }
}

impl Error for ImporterDescriptorError {}

pub struct ImportRequest<'a> {
    source: &'a AssetRecord,
    source_hash: SourceHash,
    dependency_digest: ImportDependencyDigest,
    settings_hash: ImportSettingsHash,
    profile: ImportProfile,
}

impl<'a> ImportRequest<'a> {
    #[must_use]
    pub const fn new(
        source: &'a AssetRecord,
        source_hash: SourceHash,
        dependency_digest: ImportDependencyDigest,
        settings_hash: ImportSettingsHash,
        profile: ImportProfile,
    ) -> Self {
        Self {
            source,
            source_hash,
            dependency_digest,
            settings_hash,
            profile,
        }
    }

    #[must_use]
    pub const fn source(&self) -> &AssetRecord {
        self.source
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn dependency_digest(&self) -> ImportDependencyDigest {
        self.dependency_digest
    }

    #[must_use]
    pub const fn settings_hash(&self) -> ImportSettingsHash {
        self.settings_hash
    }

    #[must_use]
    pub const fn profile(&self) -> &ImportProfile {
        &self.profile
    }

    #[must_use]
    pub fn artifact_key(
        &self,
        descriptor: &ImporterDescriptor,
        artifact_label: ArtifactLabel,
    ) -> ImportArtifactKey {
        ImportArtifactKey::new(
            self.source.stable_id(),
            self.source_hash,
            self.dependency_digest,
            descriptor.id().clone(),
            descriptor.version(),
            self.settings_hash,
            self.profile.clone(),
            descriptor.output_asset_type().clone(),
            artifact_label,
            descriptor.artifact_format_version(),
        )
    }
}

pub trait Importer: Send + Sync + 'static {
    fn descriptor(&self) -> &ImporterDescriptor;

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifactRecord, ImportError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Selection(ImporterSelectionError),
    ArtifactPath(ImportArtifactPathError),
    ImporterFailed(String),
}

impl Display for ImportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Selection(error) => Display::fmt(error, formatter),
            Self::ArtifactPath(error) => Display::fmt(error, formatter),
            Self::ImporterFailed(error) => write!(formatter, "importer failed: {error}"),
        }
    }
}

impl Error for ImportError {}

impl From<ImporterSelectionError> for ImportError {
    fn from(error: ImporterSelectionError) -> Self {
        Self::Selection(error)
    }
}

impl From<ImportArtifactPathError> for ImportError {
    fn from(error: ImportArtifactPathError) -> Self {
        Self::ArtifactPath(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImporterRegistryError {
    DuplicateImporterId {
        id: ImporterId,
    },
    DuplicateSourceExtension {
        extension: SourceExtension,
        existing_importer: ImporterId,
        new_importer: ImporterId,
    },
}

impl Display for ImporterRegistryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateImporterId { id } => {
                write!(formatter, "importer '{id}' is already registered")
            }
            Self::DuplicateSourceExtension {
                extension,
                existing_importer,
                new_importer,
            } => write!(
                formatter,
                "source extension '.{extension}' is already registered for importer '{existing_importer}', not '{new_importer}'"
            ),
        }
    }
}

impl Error for ImporterRegistryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImporterSelectionError {
    MissingSourceExtension { path: AssetPath },
    UnknownSourceExtension { extension: SourceExtension },
    UnknownImporter { id: ImporterId },
}

impl Display for ImporterSelectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSourceExtension { path } => {
                write!(formatter, "asset path '{path}' has no source extension")
            }
            Self::UnknownSourceExtension { extension } => {
                write!(formatter, "no importer is registered for '.{extension}'")
            }
            Self::UnknownImporter { id } => write!(formatter, "importer '{id}' is not registered"),
        }
    }
}

impl Error for ImporterSelectionError {}

#[derive(Default)]
pub struct ImporterRegistry {
    importers: BTreeMap<ImporterId, Box<dyn Importer>>,
    extension_to_importer: BTreeMap<SourceExtension, ImporterId>,
}

impl ImporterRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<I>(&mut self, importer: I) -> Result<(), ImporterRegistryError>
    where
        I: Importer,
    {
        let descriptor = importer.descriptor().clone();
        if self.importers.contains_key(descriptor.id()) {
            return Err(ImporterRegistryError::DuplicateImporterId {
                id: descriptor.id().clone(),
            });
        }

        for extension in descriptor.supported_extensions() {
            if let Some(existing_importer) = self.extension_to_importer.get(extension) {
                return Err(ImporterRegistryError::DuplicateSourceExtension {
                    extension: extension.clone(),
                    existing_importer: existing_importer.clone(),
                    new_importer: descriptor.id().clone(),
                });
            }
        }

        for extension in descriptor.supported_extensions() {
            self.extension_to_importer
                .insert(extension.clone(), descriptor.id().clone());
        }
        self.importers
            .insert(descriptor.id().clone(), Box::new(importer));
        Ok(())
    }

    #[must_use]
    pub fn importer(&self, id: &ImporterId) -> Option<&dyn Importer> {
        self.importers.get(id).map(Box::as_ref)
    }

    pub fn importer_or_error(
        &self,
        id: &ImporterId,
    ) -> Result<&dyn Importer, ImporterSelectionError> {
        self.importer(id)
            .ok_or_else(|| ImporterSelectionError::UnknownImporter { id: id.clone() })
    }

    #[must_use]
    pub fn importer_for_extension(&self, extension: &SourceExtension) -> Option<&dyn Importer> {
        self.extension_to_importer
            .get(extension)
            .and_then(|id| self.importer(id))
    }

    pub fn importer_for_path(
        &self,
        path: &AssetPath,
    ) -> Result<&dyn Importer, ImporterSelectionError> {
        let extension = SourceExtension::from_asset_path(path)?;
        self.importer_for_extension(&extension)
            .ok_or(ImporterSelectionError::UnknownSourceExtension { extension })
    }

    pub fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifactRecord, ImportError> {
        let importer = self.importer_for_path(request.source().path())?;
        importer.import(request)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.importers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.importers.is_empty()
    }
}

impl SourceExtension {
    pub fn from_asset_path(path: &AssetPath) -> Result<Self, ImporterSelectionError> {
        let file_name = path.as_str().rsplit('/').next().unwrap_or(path.as_str());
        let Some((_, extension)) = file_name.rsplit_once('.') else {
            return Err(ImporterSelectionError::MissingSourceExtension { path: path.clone() });
        };

        if extension.is_empty() {
            return Err(ImporterSelectionError::MissingSourceExtension { path: path.clone() });
        }

        Self::new(extension)
            .map_err(|_| ImporterSelectionError::MissingSourceExtension { path: path.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetPath, AssetSourceKind, ImportDependencyDigest, StableAssetId};

    struct MockImporter {
        descriptor: ImporterDescriptor,
    }

    impl MockImporter {
        fn new(id: &str, extension: &str, output_asset_type: &str) -> Self {
            Self {
                descriptor: ImporterDescriptor::new(
                    ImporterId::new(id).unwrap(),
                    ImporterVersion::new(1),
                    [SourceExtension::new(extension).unwrap()],
                    ImportedAssetType::new(output_asset_type).unwrap(),
                    ArtifactFormatVersion::new(1),
                )
                .unwrap(),
            }
        }
    }

    impl Importer for MockImporter {
        fn descriptor(&self) -> &ImporterDescriptor {
            &self.descriptor
        }

        fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifactRecord, ImportError> {
            let key = request.artifact_key(self.descriptor(), ArtifactLabel::default());
            ImportArtifactRecord::new(key).map_err(Into::into)
        }
    }

    fn stable_id() -> StableAssetId {
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
    }

    fn asset_record(path: &str) -> AssetRecord {
        AssetRecord::new(
            stable_id(),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Other("mock".to_string()),
        )
    }

    fn request<'a>(record: &'a AssetRecord) -> ImportRequest<'a> {
        ImportRequest::new(
            record,
            SourceHash::from_bytes(b"source"),
            ImportDependencyDigest::empty(),
            ImportSettingsHash::from_bytes(b"settings"),
            ImportProfile::default(),
        )
    }

    #[test]
    fn registry_imports_mock_non_image_artifact() {
        let mut registry = ImporterRegistry::new();
        registry
            .register(MockImporter::new("mock_importer", "mock", "mock_asset"))
            .unwrap();
        let record = asset_record("assets/source.mock");

        let artifact = registry.import(request(&record)).unwrap();

        assert_eq!(registry.len(), 1);
        assert_eq!(
            artifact.key().output_asset_type(),
            &ImportedAssetType::new("mock_asset").unwrap()
        );
        assert!(
            artifact
                .artifact_path()
                .as_str()
                .starts_with(".nara/import-cache/mock_importer/default/mock_asset/")
        );
    }

    #[test]
    fn unknown_importer_selection_returns_structured_error() {
        let registry = ImporterRegistry::new();
        let record = asset_record("assets/source.mock");

        let error = registry.import(request(&record)).unwrap_err();

        assert!(matches!(
            error,
            ImportError::Selection(ImporterSelectionError::UnknownSourceExtension { .. })
        ));
    }

    #[test]
    fn duplicate_extensions_are_rejected() {
        let mut registry = ImporterRegistry::new();
        registry
            .register(MockImporter::new("first", "mock", "mock_asset"))
            .unwrap();

        let error = registry
            .register(MockImporter::new("second", ".mock", "other_asset"))
            .unwrap_err();

        assert!(matches!(
            error,
            ImporterRegistryError::DuplicateSourceExtension { .. }
        ));
    }

    #[test]
    fn source_extensions_are_normalized_from_paths() {
        assert_eq!(
            SourceExtension::from_asset_path(&AssetPath::new("Textures/Player.PNG").unwrap())
                .unwrap()
                .as_str(),
            "png"
        );
        assert!(matches!(
            SourceExtension::from_asset_path(&AssetPath::new("textures/player").unwrap())
                .unwrap_err(),
            ImporterSelectionError::MissingSourceExtension { .. }
        ));
    }
}
