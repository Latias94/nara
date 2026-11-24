//! Fixed recovery data for replacing two existing authored sources.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    mem::size_of,
    sync::Arc,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, SourceUniverseId};
use nara_fs::{ContentDigest, RelativePath};
use nara_project::{NativeDocumentIdentity, ProjectAuthoringRecoveryDigest, ProjectAuthoringState};
use nara_scene::{SceneFileLimits, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin};
use serde::{Deserialize, Serialize};

use crate::project_host::authoring::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError, PROJECT_MANIFEST_PATH,
    recovery_storage::RecoveryPackageData,
    source_inventory::{
        SourceEncoding, SourceInventoryError, SourceInventoryManifest, product, sum,
        with_candidate_storage, with_temporary_storage,
    },
};

mod admission;
#[cfg(test)]
mod tests;

const KIND: &str = "prefab_component_apply_recovery";

#[derive(Clone, Copy)]
pub(super) struct PrefabComponentApplyRecoveryLimits {
    pub(super) encoded_bytes: ByteLimit,
    pub(super) aggregate_bytes: ByteLimit,
    pub(super) sources: ItemLimit,
    pub(super) observations: ItemLimit,
    pub(super) shape: SerdeShapeLimits,
    pub(super) scene: SceneFileLimits,
}

impl Default for PrefabComponentApplyRecoveryLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(64 * 1024 * 1024).unwrap(),
            aggregate_bytes: ByteLimit::new(32 * 1024 * 1024).unwrap(),
            sources: ItemLimit::new(4_096).unwrap(),
            observations: ItemLimit::new(16_384).unwrap(),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(64).unwrap(),
                ItemLimit::new(1_000_000).unwrap(),
                ItemLimit::new(16_384).unwrap(),
                ByteLimit::new(32 * 1024 * 1024).unwrap(),
                ByteLimit::new(48 * 1024 * 1024).unwrap(),
            ),
            scene: SceneFileLimits::default(),
        }
    }
}

/// Borrows a complete captured inventory and its two reviewed saved outputs.
/// The Project owner admits the inventory and verifies the saved generation.
pub(super) struct PrefabComponentApplyRecoveryInput<'a> {
    pub(super) ready_manifest: &'a [u8],
    pub(super) bundle_path: &'a str,
    pub(super) bundle_bytes: &'a [u8],
    pub(super) source_universe: SourceUniverseId,
    pub(super) selected_profile: Option<&'a str>,
    pub(super) complete_original_sources: &'a [RecoverySourceInput<'a>],
    pub(super) containing: RecoveryOutputInput<'a>,
    pub(super) destination: RecoveryOutputInput<'a>,
    pub(super) metadata_guards: &'a [RecoveryMetadataGuardInput<'a>],
    pub(super) roots: &'a [RecoveryRootInput<'a>],
    pub(super) missing_sidecars: &'a [&'a str],
}

#[derive(Clone, Copy)]
pub(super) struct RecoverySourceInput<'a> {
    pub(super) path: &'a str,
    pub(super) key: &'a SourceDocumentKey,
    pub(super) encoding: SourceEncoding,
    pub(super) explicit_native: bool,
    pub(super) original_disk: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryOutputInput<'a> {
    pub(super) key: &'a SourceDocumentKey,
    pub(super) output: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryMetadataGuardInput<'a> {
    pub(super) path: &'a str,
    pub(super) observed: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryRootInput<'a> {
    pub(super) path: &'a str,
    /// `None` is present. `Some("")` is missing directly beneath the project root.
    pub(super) missing_ancestor: Option<&'a str>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBlob {
    bytes: String,
    digest: ProjectAuthoringRecoveryDigest,
}

impl RecoveryBlob {
    fn copy(bytes: &[u8]) -> Result<Self, PrefabComponentApplyRecoveryError> {
        Ok(Self {
            bytes: std::str::from_utf8(bytes)
                .map_err(|_| PrefabComponentApplyRecoveryError::Utf8)?
                .to_owned(),
            digest: recovery_digest(bytes),
        })
    }

    pub(super) fn bytes(&self) -> &[u8] {
        self.bytes.as_bytes()
    }

    pub(super) fn digest(&self) -> ContentDigest {
        ContentDigest::from_parts(self.digest.length(), *self.digest.as_bytes())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBundle {
    path: String,
    blob: RecoveryBlob,
}

impl RecoveryBundle {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn blob(&self) -> &RecoveryBlob {
        &self.blob
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "SourceEncoding", rename_all = "snake_case")]
enum RecoverySourceEncoding {
    Json,
    Ron,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoverySource {
    path: String,
    key: SourceDocumentKey,
    #[serde(with = "RecoverySourceEncoding")]
    encoding: SourceEncoding,
    explicit_native: bool,
    original_disk: RecoveryBlob,
}

impl RecoverySource {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn key(&self) -> &SourceDocumentKey {
        &self.key
    }

    pub(super) fn encoding(&self) -> SourceEncoding {
        self.encoding
    }

    pub(super) fn explicit_native(&self) -> bool {
        self.explicit_native
    }

    pub(super) fn original_disk(&self) -> &RecoveryBlob {
        &self.original_disk
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryOutput {
    source_index: usize,
    output: RecoveryBlob,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryMetadataGuard {
    path: String,
    observed: RecoveryBlob,
}

impl RecoveryMetadataGuard {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn observed(&self) -> &RecoveryBlob {
        &self.observed
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryRoot {
    path: String,
    #[serde(deserialize_with = "Option::<String>::deserialize")]
    missing_ancestor: Option<String>,
}

impl RecoveryRoot {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn missing_ancestor(&self) -> Option<&str> {
        self.missing_ancestor.as_deref()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryPayload {
    ready_manifest: RecoveryBlob,
    unchanged_bundle: RecoveryBundle,
    source_universe: SourceUniverseId,
    #[serde(deserialize_with = "Option::<String>::deserialize")]
    selected_profile: Option<String>,
    complete_original_sources: Vec<RecoverySource>,
    containing: RecoveryOutput,
    destination: RecoveryOutput,
    metadata_guards: Vec<RecoveryMetadataGuard>,
    roots: Vec<RecoveryRoot>,
    missing_sidecars: Vec<String>,
}

#[must_use]
pub(super) struct PrefabComponentApplyRecoveryData {
    payload: RecoveryPayload,
    encoded: Vec<u8>,
    digest: ProjectAuthoringRecoveryDigest,
}

impl fmt::Debug for PrefabComponentApplyRecoveryData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabComponentApplyRecoveryData")
            .field("digest", &self.digest)
            .field("sources", &self.payload.complete_original_sources.len())
            .finish_non_exhaustive()
    }
}

impl PrefabComponentApplyRecoveryData {
    /// Sorts inventory tables by path and encodes the immutable package under the caller's budget.
    pub(super) fn create(
        input: PrefabComponentApplyRecoveryInput<'_>,
        limits: PrefabComponentApplyRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Arc<Self>, PrefabComponentApplyRecoveryError> {
        with_candidate_storage(budget, |budget| {
            let payload = RecoveryPayload::copy(input, limits, budget)?;
            payload.validate(budget)?;
            let encoded = admission::encode(&payload, limits, budget)?;
            budget.charge_source_bytes(0, 0, encoded.len(), 0)?;
            let digest = recovery_digest(&encoded);
            Ok(Arc::new(Self {
                payload,
                encoded,
                digest,
            }))
        })
    }

    /// Admits the concrete v1 payload and retains its exact original wire bytes for transport.
    pub(super) fn decode(
        encoded: &[u8],
        limits: PrefabComponentApplyRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Arc<Self>, PrefabComponentApplyRecoveryError> {
        with_candidate_storage(budget, |budget| {
            let payload = admission::decode(encoded, limits, budget)?;
            payload.validate(budget)?;
            budget.charge_source_bytes(0, 0, product(encoded.len(), 2)?, encoded.len())?;
            let digest = recovery_digest(encoded);
            Ok(Arc::new(Self {
                payload,
                encoded: encoded.to_vec(),
                digest,
            }))
        })
    }

    pub(super) fn unchanged_bundle(&self) -> &RecoveryBundle {
        &self.payload.unchanged_bundle
    }

    pub(super) fn selected_profile(&self) -> Option<&str> {
        self.payload.selected_profile.as_deref()
    }

    pub(super) fn complete_original_sources(&self) -> &[RecoverySource] {
        &self.payload.complete_original_sources
    }

    /// Returns the containing replacement first and the Prefab destination second.
    pub(super) fn outputs(&self) -> [(&RecoverySource, &RecoveryBlob); 2] {
        let sources = self.complete_original_sources();
        [&self.payload.containing, &self.payload.destination]
            .map(|output| (&sources[output.source_index], &output.output))
    }

    pub(super) fn metadata_guards(&self) -> &[RecoveryMetadataGuard] {
        &self.payload.metadata_guards
    }

    pub(super) fn roots(&self) -> &[RecoveryRoot] {
        &self.payload.roots
    }

    pub(super) fn missing_sidecars(&self) -> &[String] {
        &self.payload.missing_sidecars
    }
}

impl RecoveryPackageData for PrefabComponentApplyRecoveryData {
    fn encoded_bytes(&self) -> &[u8] {
        &self.encoded
    }

    fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.digest
    }

    fn source_universe(&self) -> SourceUniverseId {
        self.payload.source_universe
    }

    fn old_ready_manifest(&self) -> &[u8] {
        self.payload.ready_manifest.bytes()
    }

    fn final_ready_manifest(&self) -> &[u8] {
        self.payload.ready_manifest.bytes()
    }

    fn protected_paths(&self) -> impl Iterator<Item = &str> {
        self.payload
            .file_paths()
            .chain(self.payload.roots.iter().map(RecoveryRoot::path))
    }
}

impl RecoveryPayload {
    fn copy(
        input: PrefabComponentApplyRecoveryInput<'_>,
        limits: PrefabComponentApplyRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, PrefabComponentApplyRecoveryError> {
        admission::admit_input(&input, limits, budget)?;
        let mut complete_original_sources =
            Vec::with_capacity(input.complete_original_sources.len());
        for source in input.complete_original_sources {
            complete_original_sources.push(RecoverySource {
                path: source.path.to_owned(),
                key: (*source.key).clone(),
                encoding: source.encoding,
                explicit_native: source.explicit_native,
                original_disk: RecoveryBlob::copy(source.original_disk)?,
            });
        }
        complete_original_sources.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        let copy_output = |input_output: RecoveryOutputInput<'_>| {
            let source_index = complete_original_sources
                .iter()
                .position(|source| source.key() == input_output.key)
                .ok_or(PrefabComponentApplyRecoveryError::InvalidPair)?;
            Ok::<_, PrefabComponentApplyRecoveryError>(RecoveryOutput {
                source_index,
                output: RecoveryBlob::copy(input_output.output)?,
            })
        };
        let containing = copy_output(input.containing)?;
        let destination = copy_output(input.destination)?;

        let mut metadata_guards = Vec::with_capacity(input.metadata_guards.len());
        for guard in input.metadata_guards {
            metadata_guards.push(RecoveryMetadataGuard {
                path: guard.path.to_owned(),
                observed: RecoveryBlob::copy(guard.observed)?,
            });
        }
        metadata_guards.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        let mut roots: Vec<_> = input
            .roots
            .iter()
            .map(|root| RecoveryRoot {
                path: root.path.to_owned(),
                missing_ancestor: root.missing_ancestor.map(str::to_owned),
            })
            .collect();
        roots.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        let mut missing_sidecars: Vec<_> = input
            .missing_sidecars
            .iter()
            .map(|path| (*path).to_owned())
            .collect();
        missing_sidecars.sort_unstable();
        Ok(Self {
            ready_manifest: RecoveryBlob::copy(input.ready_manifest)?,
            unchanged_bundle: RecoveryBundle {
                path: input.bundle_path.to_owned(),
                blob: RecoveryBlob::copy(input.bundle_bytes)?,
            },
            source_universe: input.source_universe,
            selected_profile: input.selected_profile.map(str::to_owned),
            containing,
            destination,
            complete_original_sources,
            metadata_guards,
            roots,
            missing_sidecars,
        })
    }

    fn blobs(&self) -> impl Iterator<Item = &RecoveryBlob> {
        [
            &self.ready_manifest,
            self.unchanged_bundle.blob(),
            &self.containing.output,
            &self.destination.output,
        ]
        .into_iter()
        .chain(
            self.complete_original_sources
                .iter()
                .map(RecoverySource::original_disk),
        )
        .chain(
            self.metadata_guards
                .iter()
                .map(RecoveryMetadataGuard::observed),
        )
    }

    fn file_paths(&self) -> impl Iterator<Item = &str> {
        [PROJECT_MANIFEST_PATH, self.unchanged_bundle.path()]
            .into_iter()
            .chain(
                self.complete_original_sources
                    .iter()
                    .map(RecoverySource::path),
            )
            .chain(self.metadata_guards.iter().map(RecoveryMetadataGuard::path))
            .chain(self.missing_sidecars.iter().map(String::as_str))
    }

    fn directory_paths(&self) -> impl Iterator<Item = &str> {
        self.roots.iter().flat_map(|root| {
            [
                Some(root.path()),
                root.missing_ancestor().filter(|path| !path.is_empty()),
            ]
            .into_iter()
            .flatten()
        })
    }

    fn validate(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), PrefabComponentApplyRecoveryError> {
        type Error = PrefabComponentApplyRecoveryError;

        let sources = &self.complete_original_sources;
        if self.containing.source_index == self.destination.source_index
            || sources.get(self.containing.source_index).is_none()
            || !sources
                .get(self.destination.source_index)
                .is_some_and(|source| source.key.kind() == SourceDocumentKind::Prefab)
        {
            return Err(Error::InvalidPair);
        }
        let aggregate = self
            .blobs()
            .try_fold(0, |total, blob| sum(total, blob.bytes().len()))?;
        budget.charge_source_bytes(0, 0, aggregate, 0)?;
        for blob in self.blobs() {
            if recovery_digest(blob.bytes()) != blob.digest {
                return Err(Error::BlobDigestMismatch);
            }
        }
        with_temporary_storage(budget, |budget| {
            let mut manifest = SourceInventoryManifest::parse(self.ready_manifest.bytes(), budget)?;
            let manifest = manifest.value_mut();
            if manifest.authoring.state != ProjectAuthoringState::Ready
                || manifest.project.source_universe != self.source_universe
                || manifest.authoring.schema_bundle.as_deref() != Some(self.unchanged_bundle.path())
            {
                return Err(Error::Configuration);
            }
            let profile_storage = product(self.ready_manifest.bytes().len(), 128)?;
            let profile = self.selected_profile.as_deref();
            let profile_work = sum(profile_storage, profile.map_or(0, str::len))?;
            budget.charge_source_bytes(0, 0, profile_work, profile_storage)?;
            if profile.is_some_and(|name| !manifest.profiles.contains_key(name)) {
                return Err(Error::Configuration);
            }
            manifest
                .resolve_profile(profile)
                .map_err(|_| Error::Configuration)?;
            self.validate_paths_and_keys(manifest.project.native_document_identity, budget)
        })
    }

    fn validate_paths_and_keys(
        &self,
        identity: NativeDocumentIdentity,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), PrefabComponentApplyRecoveryError> {
        type Error = PrefabComponentApplyRecoveryError;

        let sources = &self.complete_original_sources;
        let keys = sources
            .iter()
            .try_fold(0, |bytes, source| sum(bytes, source.key.retained_bytes()))?;
        let paths = || self.file_paths().chain(self.directory_paths());
        let mut prefix_bytes = 0;
        let mut prefixes = 0;
        for path in paths() {
            let shape = RelativePath::preflight(path).map_err(|_| Error::InvalidPath)?;
            budget.charge_reference(path.len(), shape.components(), false)?;
            budget.charge_source_bytes(0, 0, path.len(), 0)?;
            if path.contains('\\') || path.split('/').any(|part| matches!(part, "" | "." | "..")) {
                return Err(Error::InvalidPath);
            }
            for end in path
                .match_indices('/')
                .map(|(end, _)| end)
                .chain([path.len()])
            {
                prefixes = sum(prefixes, 1)?;
                prefix_bytes = sum(prefix_bytes, end)?;
            }
        }
        let path_storage = sum(product(prefix_bytes, 3)?, product(prefixes, 256)?)?;
        let key_storage = sum(keys, product(sources.len(), 256)?)?;
        let comparisons = product((usize::BITS - prefixes.leading_zeros()) as usize + 2, 16)?;
        let path_work = sum(product(prefix_bytes, 3)?, product(prefixes, 8)?)?;
        let work = product(sum(path_work, keys)?, comparisons)?;
        budget.charge_source_bytes(0, 0, work, sum(path_storage, key_storage)?)?;
        check_sorted(sources.iter().map(RecoverySource::path))?;
        check_sorted(self.metadata_guards.iter().map(RecoveryMetadataGuard::path))?;
        check_sorted(self.roots.iter().map(RecoveryRoot::path))?;
        check_sorted(self.missing_sidecars.iter().map(String::as_str))?;

        let mut spellings = BTreeMap::new();
        for path in paths() {
            for end in path
                .match_indices('/')
                .map(|(end, _)| end)
                .chain([path.len()])
            {
                let prefix = &path[..end];
                if spellings
                    .insert(prefix.to_lowercase(), prefix)
                    .is_some_and(|old| old != prefix)
                {
                    return Err(Error::InvalidPath);
                }
            }
        }
        let mut files = BTreeSet::new();
        for path in self.file_paths() {
            if !files.insert(path) {
                return Err(Error::InvalidPath);
            }
        }
        let file_entries = files.iter().map(|path| (*path, false));
        let directory_entries = self.directory_paths().map(|path| (path, true));
        for (path, is_directory) in file_entries.chain(directory_entries) {
            if (is_directory && files.contains(path))
                || path
                    .match_indices('/')
                    .any(|(end, _)| files.contains(&path[..end]))
            {
                return Err(Error::InvalidPath);
            }
        }
        for root in &self.roots {
            if let Some(ancestor) = root.missing_ancestor()
                && !ancestor.is_empty()
                && !root
                    .path()
                    .strip_prefix(ancestor)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            {
                return Err(Error::InvalidPath);
            }
        }
        for guard in &self.metadata_guards {
            if guard.path().strip_suffix(".meta").is_none_or(str::is_empty) {
                return Err(Error::InvalidPath);
            }
        }
        for missing in &self.missing_sidecars {
            let source_path = missing.strip_suffix(".meta").ok_or(Error::InvalidPath)?;
            if sources
                .binary_search_by(|source| source.path().cmp(source_path))
                .is_err()
            {
                return Err(Error::InvalidPath);
            }
        }
        let mut origins = BTreeSet::new();
        for source in sources {
            let extension = match source.encoding() {
                SourceEncoding::Json => ".json",
                SourceEncoding::Ron => ".ron",
            };
            if !source.path().ends_with(extension) {
                return Err(Error::Encoding);
            }
            let origin = source.key.origin();
            match (identity, origin) {
                (
                    NativeDocumentIdentity::ProjectLocation,
                    SourceDocumentOrigin::ProjectLocation { universe, path },
                ) if *universe == self.source_universe && path.as_str() == source.path() => {}
                (
                    NativeDocumentIdentity::StableMetadata,
                    SourceDocumentOrigin::StableAsset { stable_id },
                ) if !stable_id.as_uuid().is_nil() => {}
                _ => return Err(Error::SourceKey),
            }
            if !origins.insert(origin) {
                return Err(Error::SourceKey);
            }
        }
        Ok(())
    }
}

fn check_sorted<'a>(
    paths: impl Iterator<Item = &'a str>,
) -> Result<(), PrefabComponentApplyRecoveryError> {
    let mut previous = None;
    for path in paths {
        if previous.is_some_and(|previous| previous >= path) {
            return Err(PrefabComponentApplyRecoveryError::InvalidPath);
        }
        previous = Some(path);
    }
    Ok(())
}

fn recovery_digest(bytes: &[u8]) -> ProjectAuthoringRecoveryDigest {
    let digest = ContentDigest::of_bytes(bytes);
    ProjectAuthoringRecoveryDigest::from_parts(digest.length(), *digest.as_bytes())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PrefabComponentApplyRecoveryError {
    #[error("the component Apply package exceeded the operation allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("the component Apply package could not admit its inventory facts")]
    Inventory(#[from] SourceInventoryError),
    #[error("the component Apply package exceeds its encoded byte limit")]
    EncodedBytesExceeded,
    #[error("the component Apply package exceeds its aggregate body byte limit")]
    AggregateBytesExceeded,
    #[error("a component Apply package member exceeds its file byte limit")]
    BlobBytesExceeded,
    #[error("the component Apply package has an invalid number of original sources")]
    SourceCount,
    #[error("the component Apply package has too many metadata, root or missing observations")]
    ObservationCount,
    #[error("the component Apply package exceeds its JSON shape limit")]
    Shape(#[source] nara_core::SerdeShapeError),
    #[error("the component Apply package has invalid JSON")]
    Syntax,
    #[error("the component Apply package could not construct its persistent header")]
    Header,
    #[error("the component Apply package has an incompatible persistent header")]
    Contract(#[source] nara_core::PersistentFileContractError),
    #[error("the component Apply package requires exact UTF-8 source bytes")]
    Utf8,
    #[error("a component Apply package member differs from its recorded byte commitment")]
    BlobDigestMismatch,
    #[error("the component Apply package does not match its Ready configuration")]
    Configuration,
    #[error("the component Apply package requires ordered, distinct, nonconflicting paths")]
    InvalidPath,
    #[error("a component Apply source has an invalid or conflicting canonical source key")]
    SourceKey,
    #[error("a component Apply source encoding differs from its existing file extension")]
    Encoding,
    #[error("component Apply requires two distinct existing sources and a Prefab destination")]
    InvalidPair,
}
