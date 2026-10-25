use std::sync::Arc;

use nara_asset::{AssetMetaFileLimits, AssetMetaIdentityCandidate, AssetPath, AssetSourceKind};
use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, RelativePath};
use nara_project::{
    EffectiveProjectPaths, EffectiveProjectSettings, NativeDocumentIdentity, ProjectPath,
};
use nara_scene::{SourceDocumentKey, SourceDocumentKind};

use super::document::AuthoringDocumentAdmissionError;
use super::{
    AuthoringProjectAdmissionBudget,
    project::{
        AuthoringProjectSourceAdmissionError, AuthoringSourceFact, source_fact_retained_bytes,
    },
};

#[derive(Clone)]
pub(crate) struct NativeDocumentSourcePolicy {
    identity: NativeDocumentIdentity,
    universe: nara_core::SourceUniverseId,
    roots: EffectiveProjectPaths,
}

impl NativeDocumentSourcePolicy {
    pub(crate) fn new(settings: &EffectiveProjectSettings) -> Self {
        Self {
            identity: settings.project.native_document_identity,
            universe: settings.project.source_universe,
            roots: settings.paths.clone(),
        }
    }

    pub(crate) fn admit(
        &self,
        root: &DirectoryCapability,
        path: &str,
        kind: SourceDocumentKind,
        expected_identity: FileIdentity,
    ) -> Result<AdmittedNativeDocumentSource, AuthoringDocumentAdmissionError> {
        self.admit_with_metadata_limit(
            root,
            path,
            path,
            None,
            kind,
            expected_identity,
            0,
            AssetMetaFileLimits::default().encoded_bytes().get(),
        )
    }

    pub(crate) fn admit_in_directory(
        &self,
        directory: &DirectoryCapability,
        source_root: &ProjectPath,
        logical_path: &str,
        kind: SourceDocumentKind,
        expected_identity: FileIdentity,
    ) -> Result<AdmittedNativeDocumentSource, AuthoringDocumentAdmissionError> {
        let path = format!("{}/{logical_path}", source_root.as_str());
        self.admit_with_metadata_limit(
            directory,
            logical_path,
            &path,
            Some(logical_path),
            kind,
            expected_identity,
            0,
            AssetMetaFileLimits::default().encoded_bytes().get(),
        )
    }

    pub(super) fn admit_budgeted(
        &self,
        root: &DirectoryCapability,
        path: &str,
        kind: SourceDocumentKind,
        expected_identity: FileIdentity,
        dependency_depth: usize,
        retained_native_handles: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<AdmittedNativeDocumentSource, AuthoringProjectSourceAdmissionError> {
        let preflight = RelativePath::preflight(path)
            .map_err(AuthoringDocumentAdmissionError::RelativePath)
            .map_err(AuthoringProjectSourceAdmissionError::NativeSource)?;
        let stable = self.identity == NativeDocumentIdentity::StableMetadata;
        // The root is already included in the caller's retained handles. Exact nested opens
        // need a parent and leaf, plus Linux's independent stored-spelling scan cursor. Metadata
        // is opened while the policy's source probe still owns its separate native handle.
        let exact_open_handles = 2 + usize::from(cfg!(target_os = "linux"));
        let handle_peak = retained_native_handles
            .saturating_add(exact_open_handles)
            .saturating_add(usize::from(stable));
        budget.admit_handle_peak(handle_peak)?;
        let (metadata_limit, aggregate_limited) = if stable {
            budget.charge_open_file(handle_peak)?;
            budget.charge_reference(
                path.len().saturating_add(".meta".len()),
                dependency_depth,
                false,
            )?;
            budget.read_limit(AssetMetaFileLimits::default().encoded_bytes().get())?
        } else {
            (0, false)
        };
        let retained_bound = std::mem::size_of::<AdmittedNativeDocumentSource>()
            .saturating_add(std::mem::size_of::<SourceDocumentKey>())
            .saturating_add(if stable {
                preflight
                    .components()
                    .saturating_mul(std::mem::size_of::<nara_fs::RelativeComponent>())
                    .saturating_add(path.len().saturating_add(".meta".len()).saturating_mul(2))
                    .saturating_add(std::mem::size_of::<AuthoringSourceFact>())
                    .saturating_add(std::mem::size_of::<Arc<AuthoringSourceFact>>())
                    .saturating_add(path.len().saturating_add(".meta".len()))
            } else {
                path.len().saturating_add(2 * std::mem::size_of::<usize>())
            });
        budget.charge_source_bytes(
            0,
            0,
            65_536_usize
                .saturating_mul(preflight.components())
                .saturating_mul(if stable { 2 } else { 1 })
                .saturating_add(metadata_limit.saturating_mul(4)),
            retained_bound,
        )?;
        let admitted = self
            .admit_with_metadata_limit(
                root,
                path,
                path,
                None,
                kind,
                expected_identity,
                dependency_depth,
                metadata_limit,
            )
            .map_err(|error| match error {
                AuthoringDocumentAdmissionError::NativeSourceMetadataTooLarge { .. } => {
                    match ProjectPath::new(format!("{path}.meta")) {
                        Ok(path) => super::project::map_source_read_error(
                            nara_fs::FsError::ByteLimitExceeded {
                                limit: metadata_limit as u64,
                            },
                            super::project::AuthoringSourceKind::AssetMetadata,
                            path,
                            aggregate_limited,
                            metadata_limit,
                            budget.maximum(),
                        ),
                        Err(error) => AuthoringProjectSourceAdmissionError::JoinedSourcePath(error),
                    }
                }
                error => AuthoringProjectSourceAdmissionError::NativeSource(error),
            })?;
        let encoded_bytes = admitted
            .metadata
            .as_ref()
            .map_or(0, |metadata| metadata.source.encoded_bytes());
        let retained_bytes = admitted
            .retained_bytes()
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.reconcile_retained_reservation(retained_bound, retained_bytes)?;
        budget.charge_source_bytes(encoded_bytes, encoded_bytes, 0, 0)?;
        Ok(admitted)
    }

    fn admit_with_metadata_limit(
        &self,
        root: &DirectoryCapability,
        open_path: &str,
        path: &str,
        declaration_path: Option<&str>,
        kind: SourceDocumentKind,
        expected_identity: FileIdentity,
        dependency_depth: usize,
        metadata_limit: usize,
    ) -> Result<AdmittedNativeDocumentSource, AuthoringDocumentAdmissionError> {
        let relative =
            RelativePath::new(open_path).map_err(AuthoringDocumentAdmissionError::RelativePath)?;
        let file = root
            .open_file_exact(&relative)
            .map_err(AuthoringDocumentAdmissionError::OpenSource)?;
        if file.identity() != expected_identity {
            return Err(AuthoringDocumentAdmissionError::SourceIdentityChanged);
        }
        let project_path = AssetPath::new(path)
            .map_err(|_| AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch)?;
        if self.identity == NativeDocumentIdentity::ProjectLocation {
            return Ok(AdmittedNativeDocumentSource {
                key: SourceDocumentKey::project_location(self.universe, project_path, kind),
                source_identity: file.identity(),
                metadata: None,
            });
        }
        let inferred_path = [
            &self.roots.assets,
            &self.roots.scenes,
            &self.roots.prefabs,
            &self.roots.scripts,
        ]
        .into_iter()
        .filter_map(|root| {
            path.strip_prefix(root.as_str())
                .and_then(|suffix| suffix.strip_prefix('/'))
                .filter(|suffix| !suffix.is_empty())
                .map(|suffix| (root.as_str().len(), suffix))
        })
        .max_by_key(|(length, _)| *length)
        .map(|(_, suffix)| suffix);
        let logical_path = declaration_path
            .or(inferred_path)
            .ok_or(AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch)?;
        let metadata_path = RelativePath::new(format!("{open_path}.meta"))
            .map_err(AuthoringDocumentAdmissionError::RelativePath)?;
        let metadata_file = root
            .open_file_exact(&metadata_path)
            .map_err(AuthoringDocumentAdmissionError::OpenSource)?;
        let bytes = metadata_file
            .read_to_end_bounded(metadata_limit as u64)
            .map_err(|error| match error {
                nara_fs::FsError::ByteLimitExceeded { limit } => {
                    AuthoringDocumentAdmissionError::NativeSourceMetadataTooLarge { maximum: limit }
                }
                error => AuthoringDocumentAdmissionError::ReadSource(error),
            })?;
        let digest = ContentDigest::of_bytes(&bytes);
        let encoded_bytes = bytes.len();
        let metadata =
            AssetMetaIdentityCandidate::decode_json_bytes_owned(bytes.into_boxed_slice())
                .map_err(|_| AuthoringDocumentAdmissionError::NativeSourceMetadataInvalid)?;
        let expected_kind = match kind {
            SourceDocumentKind::Scene => AssetSourceKind::Scene,
            SourceDocumentKind::Prefab => AssetSourceKind::Prefab,
        };
        if metadata.identity().path().as_str() != logical_path
            || metadata.identity().source_kind() != &expected_kind
        {
            return Err(AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch);
        }
        Ok(AdmittedNativeDocumentSource {
            key: SourceDocumentKey::stable_asset(metadata.identity().stable_id(), kind),
            source_identity: file.identity(),
            metadata: Some(NativeMetadataObservation {
                path: metadata_path,
                source: Arc::new(AuthoringSourceFact::native_metadata(
                    ProjectPath::new(format!("{path}.meta"))
                        .map_err(AuthoringDocumentAdmissionError::ProjectPath)?,
                    metadata_file.identity(),
                    metadata_file.parent_identity(),
                    digest,
                    encoded_bytes,
                    dependency_depth,
                )),
            }),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdmittedNativeDocumentSource {
    key: SourceDocumentKey,
    source_identity: FileIdentity,
    metadata: Option<NativeMetadataObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeMetadataObservation {
    path: RelativePath,
    source: Arc<AuthoringSourceFact>,
}

impl AdmittedNativeDocumentSource {
    pub(crate) fn key(&self) -> &SourceDocumentKey {
        &self.key
    }

    pub(crate) fn source_identity(&self) -> FileIdentity {
        self.source_identity
    }

    pub(super) fn metadata_source(&self) -> Option<&Arc<AuthoringSourceFact>> {
        self.metadata.as_ref().map(|metadata| &metadata.source)
    }

    pub(crate) fn retained_bytes(&self) -> Option<usize> {
        let mut bytes = std::mem::size_of::<Self>().checked_add(self.key.retained_bytes())?;
        if let Some(metadata) = &self.metadata {
            bytes = bytes.checked_add(source_fact_retained_bytes(&metadata.source).ok()?)?;
            for component in metadata.path.components() {
                let spelling = component.to_str()?;
                bytes = bytes
                    .checked_add(std::mem::size_of_val(component))?
                    .checked_add(spelling.len().checked_mul(2)?)?;
            }
        }
        Some(bytes)
    }

    /// Revalidates through the same capability root used for admission. Metadata paths are
    /// relative to that root, which can be the project root or an explicitly bound source root.
    pub(crate) fn revalidate(
        &self,
        root: &DirectoryCapability,
    ) -> Result<(), AuthoringDocumentAdmissionError> {
        let Some(observed) = &self.metadata else {
            return Ok(());
        };
        let file = root
            .open_file_exact(&observed.path)
            .map_err(AuthoringDocumentAdmissionError::OpenSource)?;
        let bytes = file
            .read_to_end_bounded(AssetMetaFileLimits::default().encoded_bytes().get() as u64)
            .map_err(AuthoringDocumentAdmissionError::ReadSource)?;
        if file.identity() != observed.source.identity()
            || ContentDigest::of_bytes(&bytes) != observed.source.digest()
        {
            return Err(AuthoringDocumentAdmissionError::SourceIdentityChanged);
        }
        Ok(())
    }
}
