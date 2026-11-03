//! Bounded, code-free data for one fixed Prefab extraction publication.
//!
//! This owner admits grammar, identities, configuration and exact byte commitments. It does
//! not establish complete physical membership, filesystem custody, Ready, a Schema basis or
//! semantic extraction authority. The enclosing Project owner must still verify the complete
//! stored working/output relation, injective occurrences and mixed saved generation in every
//! actual configuration. In particular, decoding this package must never rerun extraction.

use std::{fmt, mem::size_of};

use nara_core::{
    ByteLimit, DepthLimit, ItemLimit, PersistentFileContractError, SerdeShapeError,
    SerdeShapeLimits, SourceUniverseId,
};
use nara_fs::ContentDigest;
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_scene::{SceneEntityId, SceneFileEncoding, SceneFileLimits, SourceDocumentKey};
use serde::{Deserialize, Serialize};

use super::super::super::{AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError};

mod admission;
pub(super) mod storage;
mod validate;

#[cfg(test)]
mod tests;

use admission::{checked_add, checked_mul, digest, with_candidate_storage};

const KIND: &str = "prefab_extraction_recovery";
const MANIFEST_PATH: &str = "nara.toml";
const OWNER_FIXED_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy)]
pub(super) struct PrefabExtractionRecoveryLimits {
    pub(super) encoded_bytes: ByteLimit,
    pub(super) aggregate_bytes: ByteLimit,
    pub(super) sources: ItemLimit,
    pub(super) observations: ItemLimit,
    pub(super) shape: SerdeShapeLimits,
    pub(super) scene: SceneFileLimits,
}

impl Default for PrefabExtractionRecoveryLimits {
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

/// All inputs are borrowed from the actual fixed-output owner. Completeness of the original
/// inventory is a Project obligation; this constructor cannot certify an omitted file.
pub(super) struct PrefabExtractionRecoveryInput<'a> {
    pub(super) old_ready_manifest: &'a [u8],
    pub(super) final_ready_manifest: &'a [u8],
    pub(super) unchanged_bundle: RecoveryBundleInput<'a>,
    pub(super) source_universe: SourceUniverseId,
    /// The actual selected settings' profile_name, not a display name or inferred profile.
    pub(super) selected_profile: Option<&'a str>,
    pub(super) selection: RecoverySelectionInput<'a>,
    pub(super) complete_original_sources: &'a [RecoveryOriginalSourceInput<'a>],
    pub(super) q: RecoveryNewSourceInput<'a>,
    pub(super) q_sidecar: RecoveryOutputInput<'a>,
    pub(super) metadata_guards: &'a [RecoveryMetadataGuardInput<'a>],
    pub(super) roots: &'a [RecoveryRootInput<'a>],
    pub(super) missing_sidecars: &'a [&'a str],
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryBundleInput<'a> {
    pub(super) path: &'a str,
    pub(super) blob: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct RecoverySelectionInput<'a> {
    pub(super) source: &'a SourceDocumentKey,
    pub(super) root: &'a SceneEntityId,
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryOriginalSourceInput<'a> {
    pub(super) path: &'a str,
    pub(super) key: &'a SourceDocumentKey,
    pub(super) encoding: RecoverySourceEncoding,
    /// Original explicit native participation, distinct from ordinary catalogue discovery.
    pub(super) explicit_native: bool,
    pub(super) original_disk: &'a [u8],
    pub(super) reviewed_working: &'a [u8],
    pub(super) disposition: RecoverySourceDisposition,
    pub(super) output: Option<&'a [u8]>,
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryNewSourceInput<'a> {
    pub(super) path: &'a str,
    pub(super) key: &'a SourceDocumentKey,
    pub(super) encoding: RecoverySourceEncoding,
    pub(super) output: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct RecoveryOutputInput<'a> {
    pub(super) path: &'a str,
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
    /// None is present; Some("") is missing below the project root.
    pub(super) missing_ancestor: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecoverySourceEncoding {
    Json,
    Ron,
}

impl From<RecoverySourceEncoding> for SceneFileEncoding {
    fn from(value: RecoverySourceEncoding) -> Self {
        match value {
            RecoverySourceEncoding::Json => Self::Json,
            RecoverySourceEncoding::Ron => Self::Ron,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecoverySourceDisposition {
    Guard,
    SaveCurrent,
    ReplaceSource,
    RewriteClosed,
}

/// UTF-8 bytes and their exact BLAKE3 commitment. Neither part can be replaced after admission.
#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBlob {
    bytes: String,
    digest: ProjectAuthoringRecoveryDigest,
}

impl fmt::Debug for RecoveryBlob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveryBlob")
            .field("digest", &self.digest)
            .finish()
    }
}

impl RecoveryBlob {
    // The complete input copy and its UTF-8/hash work are admitted before calling this method.
    fn copy(bytes: &[u8]) -> Result<Self, PrefabExtractionRecoveryError> {
        Ok(Self {
            bytes: std::str::from_utf8(bytes)
                .map_err(|_| PrefabExtractionRecoveryError::Utf8)?
                .to_owned(),
            digest: digest(bytes),
        })
    }

    pub(super) fn bytes(&self) -> &[u8] {
        self.bytes.as_bytes()
    }

    pub(super) const fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.digest
    }

    fn validate(&self) -> Result<(), PrefabExtractionRecoveryError> {
        if digest(self.bytes()) != self.digest {
            return Err(PrefabExtractionRecoveryError::BlobDigestMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBundle {
    path: String,
    blob: RecoveryBlob,
}

impl RecoveryBundle {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn blob(&self) -> &RecoveryBlob {
        &self.blob
    }

    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryFileState {
        classify_original(&self.blob, None, observed)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoverySelection {
    source: SourceDocumentKey,
    root: SceneEntityId,
}

impl RecoverySelection {
    pub(super) const fn source(&self) -> &SourceDocumentKey {
        &self.source
    }

    pub(super) const fn root(&self) -> &SceneEntityId {
        &self.root
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryOriginalSource {
    path: String,
    key: SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    explicit_native: bool,
    original_disk: RecoveryBlob,
    reviewed_working: RecoveryBlob,
    disposition: RecoverySourceDisposition,
    output: Option<RecoveryBlob>,
}

impl RecoveryOriginalSource {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn key(&self) -> &SourceDocumentKey {
        &self.key
    }

    pub(super) const fn encoding(&self) -> RecoverySourceEncoding {
        self.encoding
    }

    pub(super) const fn explicit_native(&self) -> bool {
        self.explicit_native
    }

    pub(super) const fn original_disk(&self) -> &RecoveryBlob {
        &self.original_disk
    }

    pub(super) const fn reviewed_working(&self) -> &RecoveryBlob {
        &self.reviewed_working
    }

    pub(super) const fn disposition(&self) -> RecoverySourceDisposition {
        self.disposition
    }

    pub(super) fn output(&self) -> Option<&RecoveryBlob> {
        self.output.as_ref()
    }

    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryFileState {
        classify_original(&self.original_disk, self.output.as_ref(), observed)
    }
}

/// A new role inherently had no original file. Empty original bytes are never its sentinel.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryNewSource {
    path: String,
    key: SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    output: RecoveryBlob,
}

impl RecoveryNewSource {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn key(&self) -> &SourceDocumentKey {
        &self.key
    }

    pub(super) const fn encoding(&self) -> RecoverySourceEncoding {
        self.encoding
    }

    pub(super) const fn output(&self) -> &RecoveryBlob {
        &self.output
    }

    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryFileState {
        classify_new(&self.output, observed)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryNewSidecar {
    path: String,
    output: RecoveryBlob,
}

impl RecoveryNewSidecar {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn output(&self) -> &RecoveryBlob {
        &self.output
    }

    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryFileState {
        classify_new(&self.output, observed)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryMetadataGuard {
    path: String,
    observed: RecoveryBlob,
}

impl RecoveryMetadataGuard {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn observed(&self) -> &RecoveryBlob {
        &self.observed
    }

    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryFileState {
        classify_original(&self.observed, None, observed)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum RecoveryRootObservation {
    Present,
    Missing { ancestor: String },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryRoot {
    path: String,
    observation: RecoveryRootObservation,
}

impl RecoveryRoot {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn observation(&self) -> &RecoveryRootObservation {
        &self.observation
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryPayload {
    old_ready_manifest: RecoveryBlob,
    final_ready_manifest: RecoveryBlob,
    unchanged_bundle: RecoveryBundle,
    source_universe: SourceUniverseId,
    // Explicit null selects the default policy; absence cannot change a stored selection.
    #[serde(deserialize_with = "Option::<String>::deserialize")]
    selected_profile: Option<String>,
    selection: RecoverySelection,
    complete_original_sources: Vec<RecoveryOriginalSource>,
    q: RecoveryNewSource,
    q_sidecar: RecoveryNewSidecar,
    metadata_guards: Vec<RecoveryMetadataGuard>,
    roots: Vec<RecoveryRoot>,
    missing_sidecars: Vec<String>,
}

impl RecoveryPayload {
    fn blobs(&self) -> impl Iterator<Item = &RecoveryBlob> {
        [
            &self.old_ready_manifest,
            &self.final_ready_manifest,
            &self.unchanged_bundle.blob,
            &self.q.output,
            &self.q_sidecar.output,
        ]
        .into_iter()
        .chain(self.complete_original_sources.iter().flat_map(|source| {
            [&source.original_disk, &source.reviewed_working]
                .into_iter()
                .chain(source.output.as_ref())
        }))
        .chain(self.metadata_guards.iter().map(|guard| &guard.observed))
    }

    fn file_paths(&self) -> impl Iterator<Item = &str> {
        [
            MANIFEST_PATH,
            self.unchanged_bundle.path.as_str(),
            self.q.path.as_str(),
            self.q_sidecar.path.as_str(),
        ]
        .into_iter()
        .chain(
            self.complete_original_sources
                .iter()
                .map(|source| source.path.as_str()),
        )
        .chain(self.metadata_guards.iter().map(|guard| guard.path.as_str()))
    }
}

/// Immutable admitted data and its exact encoded allocation. No mutation or raw-byte replacement
/// API exists. A matching content state is an observation, never a replacement receipt.
pub(super) struct PrefabExtractionRecovery {
    payload: RecoveryPayload,
    encoded: Vec<u8>,
    digest: ProjectAuthoringRecoveryDigest,
}

impl fmt::Debug for PrefabExtractionRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionRecovery")
            .field("digest", &self.digest)
            .field(
                "original_sources",
                &self.payload.complete_original_sources.len(),
            )
            .field("metadata_guards", &self.payload.metadata_guards.len())
            .field("roots", &self.payload.roots.len())
            .field("missing_sidecars", &self.payload.missing_sidecars.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryFileState {
    Original,
    Output,
    Unchanged,
    NotCreated,
    MissingOriginal,
    Conflict,
}

impl PrefabExtractionRecovery {
    pub(super) fn create(
        input: PrefabExtractionRecoveryInput<'_>,
        limits: PrefabExtractionRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, PrefabExtractionRecoveryError> {
        with_candidate_storage(budget, |budget| {
            admission::admit_input(&input, limits, budget)?;
            let mut sources = Vec::with_capacity(input.complete_original_sources.len());
            for source in input.complete_original_sources {
                sources.push(RecoveryOriginalSource {
                    path: source.path.to_owned(),
                    key: source.key.clone(),
                    encoding: source.encoding,
                    explicit_native: source.explicit_native,
                    original_disk: RecoveryBlob::copy(source.original_disk)?,
                    reviewed_working: RecoveryBlob::copy(source.reviewed_working)?,
                    disposition: source.disposition,
                    output: source.output.map(RecoveryBlob::copy).transpose()?,
                });
            }
            let mut guards = Vec::with_capacity(input.metadata_guards.len());
            for guard in input.metadata_guards {
                guards.push(RecoveryMetadataGuard {
                    path: guard.path.to_owned(),
                    observed: RecoveryBlob::copy(guard.observed)?,
                });
            }
            let mut roots = Vec::with_capacity(input.roots.len());
            for root in input.roots {
                roots.push(RecoveryRoot {
                    path: root.path.to_owned(),
                    observation: match root.missing_ancestor {
                        None => RecoveryRootObservation::Present,
                        Some(ancestor) => RecoveryRootObservation::Missing {
                            ancestor: ancestor.to_owned(),
                        },
                    },
                });
            }
            let mut missing_sidecars = Vec::with_capacity(input.missing_sidecars.len());
            for path in input.missing_sidecars {
                missing_sidecars.push((*path).to_owned());
            }
            sources.sort_unstable_by(|left, right| left.path.cmp(&right.path));
            guards.sort_unstable_by(|left, right| left.path.cmp(&right.path));
            roots.sort_unstable_by(|left, right| left.path.cmp(&right.path));
            missing_sidecars.sort_unstable();
            let payload = RecoveryPayload {
                old_ready_manifest: RecoveryBlob::copy(input.old_ready_manifest)?,
                final_ready_manifest: RecoveryBlob::copy(input.final_ready_manifest)?,
                unchanged_bundle: RecoveryBundle {
                    path: input.unchanged_bundle.path.to_owned(),
                    blob: RecoveryBlob::copy(input.unchanged_bundle.blob)?,
                },
                source_universe: input.source_universe,
                selected_profile: input.selected_profile.map(str::to_owned),
                selection: RecoverySelection {
                    source: input.selection.source.clone(),
                    root: input.selection.root.clone(),
                },
                complete_original_sources: sources,
                q: RecoveryNewSource {
                    path: input.q.path.to_owned(),
                    key: input.q.key.clone(),
                    encoding: input.q.encoding,
                    output: RecoveryBlob::copy(input.q.output)?,
                },
                q_sidecar: RecoveryNewSidecar {
                    path: input.q_sidecar.path.to_owned(),
                    output: RecoveryBlob::copy(input.q_sidecar.output)?,
                },
                metadata_guards: guards,
                roots,
                missing_sidecars,
            };
            validate::validate_payload(&payload, limits, budget)?;
            let encoded = admission::encode(&payload, limits, budget)?;
            budget.charge_source_bytes(0, 0, encoded.len(), 0)?;
            let digest = digest(&encoded);
            Ok(Self {
                payload,
                encoded,
                digest,
            })
        })
    }

    pub(super) fn decode(
        encoded: &[u8],
        limits: PrefabExtractionRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, PrefabExtractionRecoveryError> {
        with_candidate_storage(budget, |budget| {
            let payload = admission::decode_payload(encoded, limits, budget)?;
            validate::validate_payload(&payload, limits, budget)?;
            budget.charge_source_bytes(0, 0, checked_mul(encoded.len(), 2)?, encoded.len())?;
            let digest = digest(encoded);
            let encoded = encoded.to_vec();
            Ok(Self {
                payload,
                encoded,
                digest,
            })
        })
    }

    pub(super) fn encoded_bytes(&self) -> &[u8] {
        &self.encoded
    }

    pub(super) const fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.digest
    }

    pub(super) const fn old_ready_manifest(&self) -> &RecoveryBlob {
        &self.payload.old_ready_manifest
    }

    pub(super) const fn final_ready_manifest(&self) -> &RecoveryBlob {
        &self.payload.final_ready_manifest
    }

    pub(super) fn classify_manifest(&self, observed: Option<ContentDigest>) -> RecoveryFileState {
        classify_original(
            &self.payload.old_ready_manifest,
            Some(&self.payload.final_ready_manifest),
            observed,
        )
    }

    pub(super) const fn unchanged_bundle(&self) -> &RecoveryBundle {
        &self.payload.unchanged_bundle
    }

    pub(super) const fn source_universe(&self) -> SourceUniverseId {
        self.payload.source_universe
    }

    pub(super) fn selected_profile(&self) -> Option<&str> {
        self.payload.selected_profile.as_deref()
    }

    pub(super) const fn selection(&self) -> &RecoverySelection {
        &self.payload.selection
    }

    pub(super) fn complete_original_sources(&self) -> &[RecoveryOriginalSource] {
        &self.payload.complete_original_sources
    }

    pub(super) const fn q(&self) -> &RecoveryNewSource {
        &self.payload.q
    }

    pub(super) const fn q_sidecar(&self) -> &RecoveryNewSidecar {
        &self.payload.q_sidecar
    }

    /// The enclosing source owner reserves the returned identity before calling. This scope
    /// charges only bounded metadata parsing, not continuing ownership of the result.
    pub(super) fn destination_identity(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<nara_asset::AssetMetaIdentity, PrefabExtractionRecoveryError> {
        let bytes = self.q_sidecar().output().bytes();
        admission::with_small_document(bytes.len(), budget, |_| {
            nara_asset::AssetMetaIdentityCandidate::decode_json_bytes(bytes)
                .map(nara_asset::AssetMetaIdentityCandidate::into_identity)
                .map_err(|_| PrefabExtractionRecoveryError::MetadataConflict)
        })
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

fn classify_original(
    original: &RecoveryBlob,
    output: Option<&RecoveryBlob>,
    observed: Option<ContentDigest>,
) -> RecoveryFileState {
    let Some(observed) = observed else {
        return RecoveryFileState::MissingOriginal;
    };
    let observed =
        ProjectAuthoringRecoveryDigest::from_parts(observed.length(), *observed.as_bytes());
    let output = output.unwrap_or(original);
    match (observed == original.digest, observed == output.digest) {
        (true, true) => RecoveryFileState::Unchanged,
        (true, false) => RecoveryFileState::Original,
        (false, true) => RecoveryFileState::Output,
        (false, false) => RecoveryFileState::Conflict,
    }
}

fn classify_new(output: &RecoveryBlob, observed: Option<ContentDigest>) -> RecoveryFileState {
    match observed {
        None => RecoveryFileState::NotCreated,
        Some(observed)
            if ProjectAuthoringRecoveryDigest::from_parts(
                observed.length(),
                *observed.as_bytes(),
            ) == output.digest =>
        {
            RecoveryFileState::Output
        }
        Some(_) => RecoveryFileState::Conflict,
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum PrefabExtractionRecoveryError {
    #[error("Prefab extraction recovery exceeded the continuing Host allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("Prefab extraction recovery shape is invalid: {0}")]
    Shape(#[source] SerdeShapeError),
    #[error("Prefab extraction recovery header contract is invalid: {0}")]
    Contract(#[source] PersistentFileContractError),
    #[error("Prefab extraction recovery exceeds its encoded-byte limit")]
    EncodedBytesExceeded,
    #[error("Prefab extraction recovery exceeds its aggregate blob-byte limit")]
    AggregateBytesExceeded,
    #[error("Prefab extraction recovery original-source count is invalid")]
    SourceCountExceeded,
    #[error("Prefab extraction recovery exceeds its observation-count limit")]
    ObservationCountExceeded,
    #[error("Prefab extraction recovery accounting overflow")]
    AccountingOverflow,
    #[error("Prefab extraction recovery header construction failed")]
    HeaderInvalid,
    #[error("Prefab extraction recovery syntax is invalid")]
    Syntax,
    #[error("Prefab extraction recovery blobs must contain UTF-8")]
    Utf8,
    #[error("Prefab extraction recovery blob commitment does not match its bytes")]
    BlobDigestMismatch,
    #[error("Prefab extraction recovery requires valid Ready manifests")]
    ManifestInvalid,
    #[error("Prefab extraction recovery changes the project configuration")]
    ConfigurationMismatch,
    #[error("Prefab extraction recovery selected profile is unavailable")]
    SelectedProfileInvalid,
    #[error("Prefab extraction recovery selection must name an original Prefab and local root")]
    SelectionInvalid,
    #[error("Prefab extraction recovery unchanged bundle is invalid")]
    BundleInvalid,
    #[error("Prefab extraction recovery contains an invalid project-relative path")]
    PathInvalid,
    #[error("Prefab extraction recovery has unsorted, duplicate or conflicting paths")]
    PathConflict,
    #[error("Prefab extraction recovery has conflicting physical source identities")]
    KeyConflict,
    #[error("Prefab extraction recovery root observations are inconsistent")]
    RootConflict,
    #[error("Prefab extraction recovery metadata observations are inconsistent")]
    MetadataConflict,
    #[error("Prefab extraction recovery Q or its identity sidecar is invalid")]
    DestinationInvalid,
    #[error("Prefab extraction recovery output presence disagrees with its disposition")]
    DispositionInvalid,
    #[error("Prefab extraction recovery source grammar is invalid")]
    SourceInvalid,
    #[error("Prefab extraction recovery output is not the source owner's canonical encoding")]
    NonCanonicalOutput,
    #[error("Prefab extraction recovery SaveCurrent output differs from reviewed working data")]
    SaveCurrentMismatch,
}
