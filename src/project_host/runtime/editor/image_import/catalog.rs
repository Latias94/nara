use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
    sync::Arc,
};

use nara_asset::{
    AssetDatabaseError, AssetMetaFileLimits, AssetMetaIdentityCandidate, AssetPath, AssetPathScope,
    AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase, StableAssetId,
};
use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, RelativePath,
};
use nara_image::ImageImportSettings;

use crate::project_host::authoring::{
    AuthoringProjectSessionSnapshot, ProjectSourceIndexEntry, ProjectSourceIndexSnapshot,
    ProjectSourceKind,
};
use crate::project_host::runtime::editor::asset_metadata::{
    AdmittedAssetMetadata, EditorAssetMetadataSnapshot,
};

const MAX_CATALOG_MEMBERS: usize = 4_096;
use crate::project_host::authoring::MAX_AUTHORING_IMAGE_IDENTITY_PATH_BYTES as MAX_RETAINED_PATH_BYTES;
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_WORK_PER_ADVANCE: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EditorImageCatalogUsage {
    members: usize,
    image_sources: usize,
    sidecars: usize,
    retained_path_bytes: usize,
    metadata_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditorImageMetadataInvalidity {
    SourceIdentityUnavailable,
    SourceUnavailable,
    SourceIdentityDrift,
    MetadataIdentityUnavailable,
    MetadataUnavailable,
    MetadataIdentityDrift,
    MetadataFormat,
    MetadataPathMismatch,
    MetadataSourceKindMismatch,
    ImageSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditorImageMetadataReceipt {
    stable_id: StableAssetId,
    identity: FileIdentity,
    digest: ContentDigest,
    settings: ImageImportSettings,
}

impl EditorImageMetadataReceipt {
    #[must_use]
    pub(super) const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub(super) const fn identity(&self) -> FileIdentity {
        self.identity
    }

    #[must_use]
    pub(super) const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub(super) const fn settings(&self) -> ImageImportSettings {
        self.settings
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EditorImageMetadataStatus {
    Missing,
    Ready(EditorImageMetadataReceipt),
    Invalid(EditorImageMetadataInvalidity),
    IdentityConflict { stable_id: StableAssetId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditorImageCatalogEntry {
    project_relative_path: Arc<str>,
    metadata_project_relative_path: Arc<str>,
    asset_path: AssetSourcePath,
    authored_path: AssetPath,
    source_identity: Option<FileIdentity>,
    metadata: EditorImageMetadataStatus,
}

impl EditorImageCatalogEntry {
    #[must_use]
    pub(super) fn project_relative_path(&self) -> &str {
        &self.project_relative_path
    }

    #[must_use]
    pub(super) fn metadata_project_relative_path(&self) -> &str {
        &self.metadata_project_relative_path
    }

    #[must_use]
    pub(super) const fn asset_path(&self) -> &AssetSourcePath {
        &self.asset_path
    }

    pub(super) const fn authored_path(&self) -> &AssetPath {
        &self.authored_path
    }

    #[must_use]
    pub(super) const fn source_identity(&self) -> Option<FileIdentity> {
        self.source_identity
    }

    #[must_use]
    pub(super) const fn metadata(&self) -> &EditorImageMetadataStatus {
        &self.metadata
    }
}

pub(super) struct EditorImageCatalogSnapshot {
    root_identity: FileIdentity,
    entries: Box<[EditorImageCatalogEntry]>,
    pub(super) metadata: Arc<EditorAssetMetadataSnapshot>,
    asset_scope: Arc<AssetPathScope>,
    prefab_scope: Arc<AssetPathScope>,
    identity_members: BTreeMap<StableAssetId, Box<[AssetSourcePath]>>,
}

impl EditorImageCatalogSnapshot {
    pub(super) fn metadata_snapshot(&self) -> &EditorAssetMetadataSnapshot {
        &self.metadata
    }
    pub(super) fn asset_identity(
        &self,
        path: &AssetSourcePath,
    ) -> Option<(AssetRecord, FileIdentity, ContentDigest)> {
        self.metadata.asset_identity(path)
    }

    pub(super) fn image_entry(&self, reference: &AssetRef) -> Option<&EditorImageCatalogEntry> {
        if !reference.is_primary() {
            return None;
        }
        self.entries.iter().find(|entry| {
            let EditorImageMetadataStatus::Ready(metadata) = entry.metadata() else {
                return false;
            };
            match reference {
                AssetRef::Path(path) => entry.authored_path() == path,
                AssetRef::StableId(stable_id) => metadata.stable_id() == *stable_id,
                AssetRef::Product(product) => metadata.stable_id() == product.source(),
            }
        })
    }

    #[must_use]
    pub(super) const fn root_identity(&self) -> FileIdentity {
        self.root_identity
    }

    #[must_use]
    pub(super) fn matches_source_index(
        &self,
        source_index: &Arc<ProjectSourceIndexSnapshot>,
    ) -> bool {
        self.metadata.matches_source_index(source_index)
    }

    #[must_use]
    pub(super) fn entry(&self, project_relative_path: &str) -> Option<&EditorImageCatalogEntry> {
        self.entries
            .binary_search_by(|entry| entry.project_relative_path().cmp(project_relative_path))
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    #[must_use]
    pub(super) fn contains_stable_id(&self, stable_id: StableAssetId) -> bool {
        self.metadata.contains_stable_id(stable_id)
    }

    #[must_use]
    pub(super) fn retains_imported_image(
        &self,
        asset_path: &AssetSourcePath,
        stable_id: StableAssetId,
        source_identity: FileIdentity,
    ) -> bool {
        self.entries.iter().any(|entry| {
            entry.asset_path() == asset_path
                && entry.source_identity() == Some(source_identity)
                && matches!(
                    entry.metadata(),
                    EditorImageMetadataStatus::Ready(metadata)
                        if metadata.stable_id() == stable_id
                )
        })
    }

    /// Reuses the admitted member set, but observes competing metadata on the worker.
    /// An old catalogue conflict is not proof that its other member still exists.
    pub(super) fn has_current_identity_conflict(
        &self,
        root: &DirectoryCapability,
        asset_path: &AssetSourcePath,
        stable_id: StableAssetId,
    ) -> Result<bool, EditorImageMetadataInvalidity> {
        let Some(members) = self.identity_members.get(&stable_id) else {
            return Ok(false);
        };
        let mut remaining = MAX_METADATA_BYTES;
        for path in members.iter().filter(|path| *path != asset_path) {
            let relative = RelativePath::new(path.meta_path())
                .map_err(|_| EditorImageMetadataInvalidity::MetadataUnavailable)?;
            let file = match root.open_file_exact(&relative) {
                Ok(file) => file,
                Err(FsError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    continue;
                }
                Err(_) => return Err(EditorImageMetadataInvalidity::MetadataUnavailable),
            };
            let limits = AssetMetaFileLimits::default();
            let limit = nara_core::ByteLimit::new(remaining.min(limits.encoded_bytes().get()))
                .ok_or(EditorImageMetadataInvalidity::MetadataUnavailable)?;
            let read = read_metadata_file(&file, limits.with_encoded_bytes(limit))?;
            remaining = remaining
                .checked_sub(read.encoded_bytes)
                .ok_or(EditorImageMetadataInvalidity::MetadataUnavailable)?;
            if read.candidate.is_ok_and(|candidate| {
                [&self.asset_scope, &self.prefab_scope]
                    .into_iter()
                    .any(|scope| {
                        scope.relative_path(path) == Some(candidate.identity().path().as_str())
                    })
                    && candidate.identity().stable_id() == stable_id
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

struct ReadMetadataCandidate {
    candidate: Result<AssetMetaIdentityCandidate, EditorImageMetadataInvalidity>,
    digest: ContentDigest,
    encoded_bytes: usize,
}

fn read_metadata_file(
    file: &FileCapability,
    limits: AssetMetaFileLimits,
) -> Result<ReadMetadataCandidate, EditorImageMetadataInvalidity> {
    let limit = u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX);
    let bytes = file
        .read_to_end_bounded(limit)
        .map_err(|_| EditorImageMetadataInvalidity::MetadataUnavailable)?;
    let encoded_bytes = bytes.len();
    let digest = ContentDigest::of_bytes(&bytes);
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
        bytes.into_boxed_slice(),
        limits,
    )
    .map_err(|_| EditorImageMetadataInvalidity::MetadataFormat);
    Ok(ReadMetadataCandidate {
        candidate,
        digest,
        encoded_bytes,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditorImageCatalogBuildError {
    SourceIndexUnavailable,
    SourceIndexStale,
    ProjectRootMismatch,
    MemberLimitExceeded,
    RetainedPathLimitExceeded,
    MetadataByteLimitExceeded,
    InvalidIndexedAssetPath,
    IdentityDatabaseInvariant,
    AlreadyCompleted,
}

#[derive(Clone, Copy)]
struct EditorImageCatalogLimits {
    max_members: usize,
    max_retained_path_bytes: usize,
    max_metadata_bytes: usize,
    max_work_per_advance: usize,
    metadata_file: AssetMetaFileLimits,
}

impl Default for EditorImageCatalogLimits {
    fn default() -> Self {
        Self {
            max_members: MAX_CATALOG_MEMBERS,
            max_retained_path_bytes: MAX_RETAINED_PATH_BYTES,
            max_metadata_bytes: MAX_METADATA_BYTES,
            max_work_per_advance: MAX_WORK_PER_ADVANCE,
            metadata_file: AssetMetaFileLimits::default(),
        }
    }
}

#[derive(Clone)]
enum CatalogWork {
    VerifySource {
        asset_path: AssetSourcePath,
        project_relative_path: Arc<str>,
        expected_identity: FileIdentity,
    },
    AdmitMetadata {
        asset_path: AssetSourcePath,
        project_relative_path: Arc<str>,
        expected_identity: FileIdentity,
        image_candidate: bool,
    },
}

struct IndexedImageSource {
    project_relative_path: Arc<str>,
    metadata_project_relative_path: Arc<str>,
    admission: SourceAdmission,
}

#[derive(Clone, Copy)]
enum SourceAdmission {
    Pending,
    Ready(FileIdentity),
    Invalid(EditorImageMetadataInvalidity),
}

struct AdmittedMetadataIdentity {
    stable_id: StableAssetId,
    identity: FileIdentity,
    digest: ContentDigest,
    settings: Result<ImageImportSettings, EditorImageMetadataInvalidity>,
}

pub(super) struct EditorImageCatalogAttempt {
    project_root: DirectoryCapability,
    source_index: Option<Arc<ProjectSourceIndexSnapshot>>,
    asset_scope: Arc<AssetPathScope>,
    prefab_scope: Arc<AssetPathScope>,
    limits: EditorImageCatalogLimits,
    sources: BTreeMap<AssetSourcePath, IndexedImageSource>,
    metadata_identities: BTreeMap<AssetSourcePath, AdmittedAssetMetadata>,
    metadata: BTreeMap<AssetSourcePath, AdmittedMetadataIdentity>,
    metadata_invalid: BTreeMap<AssetSourcePath, EditorImageMetadataInvalidity>,
    work: Vec<CatalogWork>,
    scan_cursor: usize,
    work_cursor: usize,
    usage: EditorImageCatalogUsage,
    completed: bool,
}

impl EditorImageCatalogAttempt {
    pub(super) fn begin(
        project: &AuthoringProjectSessionSnapshot,
        source_index: Arc<ProjectSourceIndexSnapshot>,
    ) -> Result<Self, EditorImageCatalogBuildError> {
        Self::begin_with_limits(project, source_index, EditorImageCatalogLimits::default())
    }

    fn begin_with_limits(
        project: &AuthoringProjectSessionSnapshot,
        source_index: Arc<ProjectSourceIndexSnapshot>,
        limits: EditorImageCatalogLimits,
    ) -> Result<Self, EditorImageCatalogBuildError> {
        let Some(current_index) = project.source_index_snapshot() else {
            return Err(EditorImageCatalogBuildError::SourceIndexUnavailable);
        };
        if !Arc::ptr_eq(&current_index, &source_index) {
            return Err(EditorImageCatalogBuildError::SourceIndexStale);
        }

        let project_root = project.project_root();
        if project_root.identity() != source_index.root_identity() {
            return Err(EditorImageCatalogBuildError::ProjectRootMismatch);
        }

        let attempt = Self {
            project_root,
            source_index: Some(source_index),
            asset_scope: project.asset_scope(),
            prefab_scope: project.prefab_scope(),
            limits,
            sources: BTreeMap::new(),
            metadata_identities: BTreeMap::new(),
            metadata: BTreeMap::new(),
            metadata_invalid: BTreeMap::new(),
            work: Vec::new(),
            scan_cursor: 0,
            work_cursor: 0,
            usage: EditorImageCatalogUsage::default(),
            completed: false,
        };
        Ok(attempt)
    }

    pub(super) fn advance(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<Option<EditorImageCatalogSnapshot>, EditorImageCatalogBuildError> {
        if self.completed {
            return Err(EditorImageCatalogBuildError::AlreadyCompleted);
        }
        self.revalidate_source_index(project)?;

        let mut remaining = self.limits.max_work_per_advance;
        while remaining > 0 && self.scan_cursor < self.source_entry_count()? {
            let entry = self.source_entry(self.scan_cursor)?.clone();
            self.scan_cursor += 1;
            remaining -= 1;
            let asset_scope = Arc::clone(&self.asset_scope);
            match entry.kind() {
                ProjectSourceKind::AssetSource if is_png_project_path(entry.relative_path()) => {
                    self.record_source(&entry, &asset_scope)?;
                }
                ProjectSourceKind::AssetMeta => {
                    self.record_metadata(&entry, &asset_scope)?;
                }
                ProjectSourceKind::Unknown => {
                    let source = entry
                        .relative_path()
                        .strip_suffix(".meta")
                        .and_then(|path| AssetSourcePath::new(path).ok());
                    if source
                        .as_ref()
                        .is_some_and(|path| self.prefab_scope.relative_path(path).is_some())
                    {
                        self.record_metadata(&entry, &asset_scope)?;
                    }
                }
                _ => {}
            }
        }
        while remaining > 0 && self.work_cursor < self.work.len() {
            let work = self.work[self.work_cursor].clone();
            self.process(work)?;
            self.work_cursor += 1;
            remaining -= 1;
        }
        if self.scan_cursor < self.source_entry_count()? || self.work_cursor < self.work.len() {
            return Ok(None);
        }

        self.completed = true;
        self.publish().map(Some)
    }

    fn source_entry_count(&self) -> Result<usize, EditorImageCatalogBuildError> {
        self.source_index
            .as_ref()
            .map(|index| index.entries().len())
            .ok_or(EditorImageCatalogBuildError::AlreadyCompleted)
    }

    fn source_entry(
        &self,
        index: usize,
    ) -> Result<&ProjectSourceIndexEntry, EditorImageCatalogBuildError> {
        self.source_index
            .as_ref()
            .and_then(|source_index| source_index.entries().get(index))
            .ok_or(EditorImageCatalogBuildError::AlreadyCompleted)
    }

    fn revalidate_source_index(
        &self,
        project: &AuthoringProjectSessionSnapshot,
    ) -> Result<(), EditorImageCatalogBuildError> {
        let Some(current_index) = project.source_index_snapshot() else {
            return Err(EditorImageCatalogBuildError::SourceIndexUnavailable);
        };
        let expected_index = self
            .source_index
            .as_ref()
            .ok_or(EditorImageCatalogBuildError::AlreadyCompleted)?;
        if !Arc::ptr_eq(&current_index, expected_index) {
            return Err(EditorImageCatalogBuildError::SourceIndexStale);
        }
        Ok(())
    }

    fn record_source(
        &mut self,
        entry: &ProjectSourceIndexEntry,
        asset_scope: &AssetPathScope,
    ) -> Result<(), EditorImageCatalogBuildError> {
        let asset_path = project_path_to_source_path(entry.relative_path(), asset_scope)?;
        self.charge_member(entry.relative_path(), asset_path.as_str())?;
        // Publication retains an authored alias alongside the canonical catalogue key.
        self.charge_path_bytes(
            asset_scope
                .relative_path(&asset_path)
                .expect("the admitted source is inside its Assets scope")
                .len(),
        )?;
        self.usage.image_sources = self.usage.image_sources.saturating_add(1);

        let metadata_project_relative_path: Arc<str> =
            Arc::from(format!("{}.meta", entry.relative_path()));
        self.charge_path_bytes(metadata_project_relative_path.len())?;
        let admission = match entry.scoped_identity() {
            Some(expected_identity) => {
                self.work.push(CatalogWork::VerifySource {
                    asset_path: asset_path.clone(),
                    project_relative_path: Arc::clone(entry.relative_path_arc()),
                    expected_identity,
                });
                SourceAdmission::Pending
            }
            None => {
                SourceAdmission::Invalid(EditorImageMetadataInvalidity::SourceIdentityUnavailable)
            }
        };
        if self
            .sources
            .insert(
                asset_path,
                IndexedImageSource {
                    project_relative_path: Arc::clone(entry.relative_path_arc()),
                    metadata_project_relative_path,
                    admission,
                },
            )
            .is_some()
        {
            return Err(EditorImageCatalogBuildError::InvalidIndexedAssetPath);
        }
        Ok(())
    }

    fn record_metadata(
        &mut self,
        entry: &ProjectSourceIndexEntry,
        asset_scope: &AssetPathScope,
    ) -> Result<(), EditorImageCatalogBuildError> {
        let source_project_path = entry
            .relative_path()
            .strip_suffix(".meta")
            .ok_or(EditorImageCatalogBuildError::InvalidIndexedAssetPath)?;
        let asset_path = project_path_to_source_path(source_project_path, asset_scope)
            .or_else(|_| project_path_to_source_path(source_project_path, &self.prefab_scope))?;
        let image_candidate = asset_scope.relative_path(&asset_path).is_some()
            && is_png_project_path(source_project_path);
        self.charge_member(entry.relative_path(), asset_path.as_str())?;
        self.usage.sidecars = self.usage.sidecars.saturating_add(1);

        match entry.scoped_identity() {
            Some(expected_identity) => self.work.push(CatalogWork::AdmitMetadata {
                asset_path,
                project_relative_path: Arc::clone(entry.relative_path_arc()),
                expected_identity,
                image_candidate,
            }),
            None => {
                if image_candidate {
                    self.metadata_invalid.insert(
                        asset_path,
                        EditorImageMetadataInvalidity::MetadataIdentityUnavailable,
                    );
                }
            }
        }
        Ok(())
    }

    fn charge_member(
        &mut self,
        project_relative_path: &str,
        asset_path: &str,
    ) -> Result<(), EditorImageCatalogBuildError> {
        let next_members = self
            .usage
            .members
            .checked_add(1)
            .ok_or(EditorImageCatalogBuildError::MemberLimitExceeded)?;
        if next_members > self.limits.max_members {
            return Err(EditorImageCatalogBuildError::MemberLimitExceeded);
        }
        self.usage.members = next_members;
        let path_bytes = project_relative_path
            .len()
            .checked_add(asset_path.len())
            .ok_or(EditorImageCatalogBuildError::RetainedPathLimitExceeded)?;
        self.charge_path_bytes(path_bytes)
    }

    fn charge_path_bytes(&mut self, bytes: usize) -> Result<(), EditorImageCatalogBuildError> {
        let next = self
            .usage
            .retained_path_bytes
            .checked_add(bytes)
            .ok_or(EditorImageCatalogBuildError::RetainedPathLimitExceeded)?;
        if next > self.limits.max_retained_path_bytes {
            return Err(EditorImageCatalogBuildError::RetainedPathLimitExceeded);
        }
        self.usage.retained_path_bytes = next;
        Ok(())
    }

    fn charge_metadata_bytes(&mut self, bytes: usize) -> Result<(), EditorImageCatalogBuildError> {
        let next = self
            .usage
            .metadata_bytes
            .checked_add(bytes)
            .ok_or(EditorImageCatalogBuildError::MetadataByteLimitExceeded)?;
        if next > self.limits.max_metadata_bytes {
            return Err(EditorImageCatalogBuildError::MetadataByteLimitExceeded);
        }
        self.usage.metadata_bytes = next;
        Ok(())
    }

    fn process(&mut self, work: CatalogWork) -> Result<(), EditorImageCatalogBuildError> {
        match work {
            CatalogWork::VerifySource {
                asset_path,
                project_relative_path,
                expected_identity,
            } => {
                let admission = self.verify_source(&project_relative_path, expected_identity);
                let source = self
                    .sources
                    .get_mut(&asset_path)
                    .expect("source verification work refers to one indexed PNG source");
                source.admission = admission;
                Ok(())
            }
            CatalogWork::AdmitMetadata {
                asset_path,
                project_relative_path,
                expected_identity,
                image_candidate,
            } => self.admit_metadata(
                asset_path,
                &project_relative_path,
                expected_identity,
                image_candidate,
            ),
        }
    }

    fn verify_source(
        &self,
        project_relative_path: &str,
        expected_identity: FileIdentity,
    ) -> SourceAdmission {
        let Ok(relative) = RelativePath::new(project_relative_path) else {
            return SourceAdmission::Invalid(EditorImageMetadataInvalidity::SourceUnavailable);
        };
        let Ok(source) = self.project_root.open_file(&relative) else {
            return SourceAdmission::Invalid(EditorImageMetadataInvalidity::SourceUnavailable);
        };
        if source.identity() != expected_identity {
            return SourceAdmission::Invalid(EditorImageMetadataInvalidity::SourceIdentityDrift);
        }
        SourceAdmission::Ready(expected_identity)
    }

    fn admit_metadata(
        &mut self,
        asset_path: AssetSourcePath,
        project_relative_path: &str,
        expected_identity: FileIdentity,
        image_candidate: bool,
    ) -> Result<(), EditorImageCatalogBuildError> {
        let admission = self.read_metadata_candidate(project_relative_path, expected_identity)?;
        let (candidate, digest) = match admission {
            Ok(candidate) => candidate,
            Err(reason) => {
                if image_candidate {
                    self.metadata_invalid.insert(asset_path, reason);
                }
                return Ok(());
            }
        };
        let scope = [&self.asset_scope, &self.prefab_scope]
            .into_iter()
            .find(|scope| {
                scope.relative_path(&asset_path) == Some(candidate.identity().path().as_str())
            });
        if !scope.is_some_and(|scope| {
            candidate.identity().clone().into_record(scope).path() == &asset_path
        }) {
            if image_candidate {
                self.metadata_invalid.insert(
                    asset_path,
                    EditorImageMetadataInvalidity::MetadataPathMismatch,
                );
            }
            return Ok(());
        }

        let candidate = Arc::new(candidate);
        let stable_id = candidate.identity().stable_id();
        let source_kind = candidate.identity().source_kind();
        self.metadata_identities.insert(
            asset_path.clone(),
            AdmittedAssetMetadata {
                candidate: Arc::clone(&candidate),
                identity: expected_identity,
                digest,
            },
        );
        if !image_candidate {
            return Ok(());
        }
        if source_kind != &AssetSourceKind::Image {
            self.metadata_invalid.insert(
                asset_path,
                EditorImageMetadataInvalidity::MetadataSourceKindMismatch,
            );
            return Ok(());
        }

        let settings = candidate
            .admit::<ImageImportSettings>()
            .map(|candidate| *candidate.settings())
            .map_err(|_| EditorImageMetadataInvalidity::ImageSettings);
        self.metadata.insert(
            asset_path,
            AdmittedMetadataIdentity {
                stable_id,
                identity: expected_identity,
                digest,
                settings,
            },
        );
        Ok(())
    }

    fn read_metadata_candidate(
        &mut self,
        project_relative_path: &str,
        expected_identity: FileIdentity,
    ) -> Result<
        Result<(AssetMetaIdentityCandidate, ContentDigest), EditorImageMetadataInvalidity>,
        EditorImageCatalogBuildError,
    > {
        let relative = match RelativePath::new(project_relative_path) {
            Ok(relative) => relative,
            Err(_) => return Ok(Err(EditorImageMetadataInvalidity::MetadataUnavailable)),
        };
        let metadata = match self.project_root.open_file(&relative) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(Err(EditorImageMetadataInvalidity::MetadataUnavailable)),
        };
        if metadata.identity() != expected_identity {
            return Ok(Err(EditorImageMetadataInvalidity::MetadataIdentityDrift));
        }
        let read = match read_metadata_file(&metadata, self.limits.metadata_file) {
            Ok(read) => read,
            Err(reason) => return Ok(Err(reason)),
        };
        self.charge_metadata_bytes(read.encoded_bytes)?;
        Ok(read.candidate.map(|candidate| (candidate, read.digest)))
    }

    fn publish(&mut self) -> Result<EditorImageCatalogSnapshot, EditorImageCatalogBuildError> {
        let metadata = mem::take(&mut self.metadata);
        let metadata_invalid = mem::take(&mut self.metadata_invalid);
        let (conflicts, known_stable_ids) =
            find_identity_conflicts(self.metadata_identities.iter().map(|(path, metadata)| {
                let identity = metadata.candidate.identity();
                (path, identity.stable_id(), identity.source_kind())
            }))?;
        let admitted_identities = mem::take(&mut self.metadata_identities);
        let mut identity_members = BTreeMap::<StableAssetId, Vec<AssetSourcePath>>::new();
        for (path, metadata) in &admitted_identities {
            identity_members
                .entry(metadata.candidate.identity().stable_id())
                .or_default()
                .push(path.clone());
        }
        let sources = mem::take(&mut self.sources);
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(sources.len())
            .map_err(|_| EditorImageCatalogBuildError::MemberLimitExceeded)?;

        for (asset_path, source) in sources {
            let source_identity = match source.admission {
                SourceAdmission::Ready(identity) => Some(identity),
                SourceAdmission::Pending => {
                    return Err(EditorImageCatalogBuildError::IdentityDatabaseInvariant);
                }
                SourceAdmission::Invalid(_) => None,
            };
            let metadata_status = if let Some(stable_id) = conflicts.get(&asset_path).copied() {
                EditorImageMetadataStatus::IdentityConflict { stable_id }
            } else {
                match source.admission {
                    SourceAdmission::Invalid(reason) => EditorImageMetadataStatus::Invalid(reason),
                    SourceAdmission::Ready(_) => match metadata.get(&asset_path) {
                        Some(metadata) => match metadata.settings {
                            Ok(settings) => {
                                EditorImageMetadataStatus::Ready(EditorImageMetadataReceipt {
                                    stable_id: metadata.stable_id,
                                    identity: metadata.identity,
                                    digest: metadata.digest,
                                    settings,
                                })
                            }
                            Err(reason) => EditorImageMetadataStatus::Invalid(reason),
                        },
                        None => metadata_invalid.get(&asset_path).copied().map_or(
                            EditorImageMetadataStatus::Missing,
                            EditorImageMetadataStatus::Invalid,
                        ),
                    },
                    SourceAdmission::Pending => unreachable!(),
                }
            };
            entries.push(EditorImageCatalogEntry {
                project_relative_path: source.project_relative_path,
                metadata_project_relative_path: source.metadata_project_relative_path,
                authored_path: self
                    .asset_scope
                    .relativize(&asset_path)
                    .ok_or(EditorImageCatalogBuildError::InvalidIndexedAssetPath)?,
                asset_path,
                source_identity,
                metadata: metadata_status,
            });
        }
        entries.sort_by(|left, right| left.project_relative_path.cmp(&right.project_relative_path));

        let source_index = self
            .source_index
            .take()
            .ok_or(EditorImageCatalogBuildError::AlreadyCompleted)?;
        Ok(EditorImageCatalogSnapshot {
            root_identity: source_index.root_identity(),
            entries: entries.into_boxed_slice(),
            asset_scope: Arc::clone(&self.asset_scope),
            prefab_scope: Arc::clone(&self.prefab_scope),
            identity_members: identity_members
                .into_iter()
                .map(|(id, members)| (id, members.into_boxed_slice()))
                .collect(),
            metadata: Arc::new(EditorAssetMetadataSnapshot::new(
                source_index,
                admitted_identities,
                known_stable_ids,
                conflicts,
            )),
        })
    }
}

fn find_identity_conflicts<'a>(
    identities: impl Iterator<Item = (&'a AssetSourcePath, StableAssetId, &'a AssetSourceKind)>,
) -> Result<
    (
        BTreeMap<AssetSourcePath, StableAssetId>,
        BTreeSet<StableAssetId>,
    ),
    EditorImageCatalogBuildError,
> {
    let mut duplicate_probe = ProjectAssetDatabase::default();
    let mut conflicts = BTreeMap::new();
    let mut known_stable_ids = BTreeSet::new();
    for (path, stable_id, source_kind) in identities {
        known_stable_ids.insert(stable_id);
        let record = AssetRecord::new(stable_id, path.clone(), source_kind.clone());
        match duplicate_probe.insert(record) {
            Ok(()) => {}
            Err(AssetDatabaseError::DuplicateStableId {
                stable_id,
                existing_path,
                new_path,
            }) => {
                conflicts.insert(existing_path, stable_id);
                conflicts.insert(new_path, stable_id);
            }
            Err(_) => return Err(EditorImageCatalogBuildError::IdentityDatabaseInvariant),
        }
    }
    Ok((conflicts, known_stable_ids))
}

fn project_path_to_source_path(
    project_relative_path: &str,
    scope: &AssetPathScope,
) -> Result<AssetSourcePath, EditorImageCatalogBuildError> {
    let path = AssetSourcePath::new(project_relative_path)
        .map_err(|_| EditorImageCatalogBuildError::InvalidIndexedAssetPath)?;
    scope
        .relative_path(&path)
        .ok_or(EditorImageCatalogBuildError::InvalidIndexedAssetPath)?;
    Ok(path)
}

fn is_png_project_path(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("png"))
}

#[cfg(test)]
fn is_png_metadata_project_path(path: &str) -> bool {
    path.strip_suffix(".meta").is_some_and(is_png_project_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable_id(value: &str) -> StableAssetId {
        StableAssetId::parse_str(value).unwrap()
    }

    #[test]
    fn asset_paths_require_the_exact_assets_root_boundary() {
        let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
        assert_eq!(
            project_path_to_source_path("assets/textures/player.png", &scope)
                .unwrap()
                .as_str(),
            "assets/textures/player.png"
        );
        assert_eq!(
            project_path_to_source_path("assets2/player.png", &scope),
            Err(EditorImageCatalogBuildError::InvalidIndexedAssetPath)
        );
        assert_eq!(
            project_path_to_source_path("assets", &scope),
            Err(EditorImageCatalogBuildError::InvalidIndexedAssetPath)
        );
    }

    #[test]
    fn png_sources_and_sidecars_share_the_image_extension_contract() {
        assert!(is_png_project_path("assets/icon.png"));
        assert!(is_png_project_path("assets/icon.PNG"));
        assert!(is_png_metadata_project_path("assets/icon.PNG.meta"));
        assert!(!is_png_metadata_project_path("assets/icon.png.META"));
        assert!(!is_png_project_path("assets/icon.jpg"));
    }

    #[test]
    fn duplicate_stable_ids_conflict_on_both_paths_and_remain_reserved() {
        let duplicate = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
        let first_path = AssetSourcePath::new("assets/first.png").unwrap();
        let second_path = AssetSourcePath::new("assets/second.png").unwrap();
        let first = AssetSourceKind::Image;
        let second = AssetSourceKind::Other("audio".to_owned());
        let identities = [
            (&first_path, duplicate, &first),
            (&second_path, duplicate, &second),
        ];
        let (conflicts, known) = find_identity_conflicts(identities.into_iter()).unwrap();

        assert_eq!(conflicts.get(&first_path), Some(&duplicate));
        assert_eq!(conflicts.get(&second_path), Some(&duplicate));
        assert_eq!(known, BTreeSet::from([duplicate]));
    }
}
