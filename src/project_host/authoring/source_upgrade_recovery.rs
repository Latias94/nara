//! Bounded, code-free recovery data for one reviewed authored-field upgrade.
//!
//! Admission here proves the persisted data's format, configuration, native source identities,
//! and actual catalog successor. The enclosing operation supplies the **complete reviewed**
//! inventory and proves the working-to-successor transformation through the source owners. It
//! must re-enumerate with fresh root capabilities on recovery. Paths and negative observations
//! survive a process; FileIdentity, nominal bases, Adapter receipts and publication witnesses do
//! not. Nothing in this module opens files, executes conversion code or admits a ready project.

use std::{
    collections::BTreeSet,
    fmt,
    io::{self, Write},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{AssetMetaIdentityCandidate, AssetPathScope, AssetSourceKind, AssetSourcePath};
use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileEnvelope,
    PersistentFileHeader, SerdeShapeError, SerdeShapeLimits, SerdeShapePreflightError,
    SourceUniverseId, preflight_serde_shape,
};
use nara_fs::{ContentDigest, RelativePath};
use nara_project::{
    NativeDocumentIdentity, ProjectAuthoringRecovery, ProjectAuthoringRecoveryDigest,
    ProjectAuthoringState, ProjectManifest,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleLimits, ComponentFieldId, ComponentFieldSchema,
    ComponentPropertyPath, ComponentPropertyReadBudget, ComponentPropertyStep, ComponentSchema,
    ComponentShapeMember, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    __private::{
        AuthoredFieldUpgrade, AuthoringAdapterReceipt, canonical_prefab_source_matches,
        canonical_scene_source_matches,
    },
    AuthoringSchemaBasis, PrefabDocumentCandidate, SceneDocumentCandidate, SceneFileEncoding,
    SceneFileLimits, SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin,
};
use serde::{Deserialize, Serialize};

use super::{AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError};

#[cfg(test)]
mod tests;

const KIND: &str = "authored_upgrade_recovery";
const MANIFEST_PATH: &str = "nara.toml";
const OWNER_FIXED_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy)]
pub(super) struct SourceUpgradeRecoveryLimits {
    pub(super) encoded_bytes: ByteLimit,
    pub(super) aggregate_bytes: ByteLimit,
    pub(super) sources: ItemLimit,
    pub(super) observations: ItemLimit,
    pub(super) shape: SerdeShapeLimits,
    pub(super) scene: SceneFileLimits,
}

impl Default for SourceUpgradeRecoveryLimits {
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

/// Borrowed reviewed data, already validated for complete physical membership by the enclosing
/// operation. Supplying a subset cannot establish ownership of the whole project here.
pub(super) struct SourceUpgradeRecoveryInput<'a> {
    pub(super) old_ready_manifest: &'a [u8],
    pub(super) future_ready_manifest: &'a [u8],
    pub(super) bundle_path: &'a str,
    pub(super) old_bundle: &'a [u8],
    pub(super) successor_bundle: &'a [u8],
    pub(super) component: &'a ComponentTypeId,
    pub(super) field: &'a ComponentFieldId,
    pub(super) successor_property: &'a ComponentPropertyPath,
    pub(super) members: &'a [SourceUpgradeRecoveryMemberInput<'a>],
    pub(super) metadata: &'a [SourceUpgradeRecoveryMetadataInput<'a>],
    pub(super) roots: &'a [SourceUpgradeRecoveryRootInput<'a>],
    pub(super) missing_sidecars: &'a [&'a str],
}

#[derive(Clone, Copy)]
pub(super) struct SourceUpgradeRecoveryMemberInput<'a> {
    pub(super) path: &'a str,
    pub(super) key: &'a SourceDocumentKey,
    pub(super) encoding: RecoverySourceEncoding,
    pub(super) old_disk: &'a [u8],
    pub(super) reviewed_working: &'a [u8],
    pub(super) successor: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct SourceUpgradeRecoveryMetadataInput<'a> {
    pub(super) path: &'a str,
    pub(super) observed_bytes: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct SourceUpgradeRecoveryRootInput<'a> {
    pub(super) path: &'a str,
    /// None means observed present; Some("") means missing below the project root.
    pub(super) missing_ancestor: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecoverySourceEncoding {
    Json,
    Ron,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum RecoveryRootObservation {
    Present,
    Missing { ancestor_path: String },
}

/// A negative observation contains no durable claim about a native directory incarnation.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryRoot {
    pub(super) path: String,
    pub(super) observation: RecoveryRootObservation,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecoveryHistoryPolicy {
    SaveReviewedWorkingCopiesAndStartNewHistory,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryBytes {
    bytes: String,
    digest: ProjectAuthoringRecoveryDigest,
}

impl RecoveryBytes {
    // Storage was admitted by input_retained_bytes before any input is copied.
    fn copy(bytes: &[u8]) -> Result<Self, SourceUpgradeRecoveryError> {
        Ok(Self {
            bytes: text(bytes)?.to_owned(),
            digest: digest(bytes),
        })
    }

    pub(super) fn bytes(&self) -> &[u8] {
        self.bytes.as_bytes()
    }

    pub(super) const fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.digest
    }

    fn validate(&self) -> Result<(), SourceUpgradeRecoveryError> {
        if digest(self.bytes()) != self.digest {
            return Err(SourceUpgradeRecoveryError::MemberDigestMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryReplacement {
    pub(super) path: String,
    pub(super) old_disk: RecoveryBytes,
    pub(super) successor: RecoveryBytes,
}

impl RecoveryReplacement {
    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryMemberState {
        classify(&self.old_disk, &self.successor, observed)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryMember {
    pub(super) path: String,
    pub(super) key: SourceDocumentKey,
    pub(super) encoding: RecoverySourceEncoding,
    pub(super) old_disk: RecoveryBytes,
    pub(super) reviewed_working: RecoveryBytes,
    pub(super) successor: RecoveryBytes,
}

impl RecoveryMember {
    /// Classifies a fresh actual byte digest. New/Unchanged require no conversion or write;
    /// Old may only bind the already stored successor after fresh filesystem admission.
    pub(super) fn classify(&self, observed: Option<ContentDigest>) -> RecoveryMemberState {
        classify(&self.old_disk, &self.successor, observed)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryMetadata {
    pub(super) path: String,
    pub(super) observed: RecoveryBytes,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryField {
    component: ComponentTypeId,
    field: ComponentFieldId,
    successor_property: ComponentPropertyPath,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryPayload {
    source_universe: SourceUniverseId,
    history_policy: RecoveryHistoryPolicy,
    field: RecoveryField,
    manifest: RecoveryReplacement,
    bundle: RecoveryReplacement,
    members: Vec<RecoveryMember>,
    metadata: Vec<RecoveryMetadata>,
    roots: Vec<RecoveryRoot>,
    missing_sidecars: Vec<String>,
}

/// Immutable bytes identify this operation. No process-local authority is serialized or retained.
#[derive(Debug)]
pub(super) struct SourceUpgradeRecovery {
    payload: RecoveryPayload,
    encoded: Vec<u8>,
    digest: ProjectAuthoringRecoveryDigest,
}

/// Returns the still-owned package when sharing cannot admit its control block.
#[derive(Debug)]
pub(super) struct SourceUpgradeRecoveryShareFailure {
    pub(super) recovery: SourceUpgradeRecovery,
    pub(super) error: SourceUpgradeRecoveryError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryMemberState {
    Old,
    New,
    /// Exact old/new equality proves neither that a replacement occurred nor that one is needed.
    Unchanged,
    Missing,
    Conflict,
}

impl SourceUpgradeRecovery {
    pub(super) fn prepare(
        input: SourceUpgradeRecoveryInput<'_>,
        limits: SourceUpgradeRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeRecoveryError> {
        with_candidate_storage(budget, |budget| {
            check_counts(
                input.members.len(),
                input.metadata.len(),
                input.roots.len(),
                input.missing_sidecars.len(),
                limits,
            )?;
            let aggregate = input_aggregate_bytes(&input)?;
            check_aggregate(aggregate, limits)?;
            // Count every stored string/key/selector and collection slot before Clone/to_owned.
            let retained = input_retained_bytes(&input)?;
            // Admission work is cumulative even when later parsing or semantic validation refuses.
            // Only explicitly temporary reservations are refunded; failure publishes no package.
            budget.charge_source_bytes(0, 0, 0, retained)?;
            let source_universe = with_owner(input.old_ready_manifest.len(), budget, |_| {
                parse_ready(input.old_ready_manifest)
                    .map(|manifest| manifest.project.source_universe)
            })?;
            // Admit UTF-8 validation, copying, hashing and collection ordering before the first
            // owned input is created. Later semantic validation performs independent passes; its
            // allowance cannot pay for construction that has already happened on a refused input.
            budget.charge_source_bytes(0, 0, input_copy_work(&input, aggregate)?, 0)?;
            let mut members = Vec::with_capacity(input.members.len());
            for member in input.members {
                members.push(RecoveryMember {
                    path: member.path.to_owned(),
                    key: member.key.clone(),
                    encoding: member.encoding,
                    old_disk: RecoveryBytes::copy(member.old_disk)?,
                    reviewed_working: RecoveryBytes::copy(member.reviewed_working)?,
                    successor: RecoveryBytes::copy(member.successor)?,
                });
            }
            let mut metadata = Vec::with_capacity(input.metadata.len());
            for observed in input.metadata {
                metadata.push(RecoveryMetadata {
                    path: observed.path.to_owned(),
                    observed: RecoveryBytes::copy(observed.observed_bytes)?,
                });
            }
            let mut roots = Vec::with_capacity(input.roots.len());
            for root in input.roots {
                roots.push(RecoveryRoot {
                    path: root.path.to_owned(),
                    observation: match root.missing_ancestor {
                        Some(path) => RecoveryRootObservation::Missing {
                            ancestor_path: path.to_owned(),
                        },
                        None => RecoveryRootObservation::Present,
                    },
                });
            }
            let mut missing_sidecars = Vec::with_capacity(input.missing_sidecars.len());
            for path in input.missing_sidecars {
                missing_sidecars.push((*path).to_owned());
            }
            let payload = RecoveryPayload {
                source_universe,
                history_policy: RecoveryHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory,
                field: RecoveryField {
                    component: input.component.clone(),
                    field: input.field.clone(),
                    successor_property: input.successor_property.clone(),
                },
                manifest: RecoveryReplacement {
                    path: MANIFEST_PATH.to_owned(),
                    old_disk: RecoveryBytes::copy(input.old_ready_manifest)?,
                    successor: RecoveryBytes::copy(input.future_ready_manifest)?,
                },
                bundle: RecoveryReplacement {
                    path: input.bundle_path.to_owned(),
                    old_disk: RecoveryBytes::copy(input.old_bundle)?,
                    successor: RecoveryBytes::copy(input.successor_bundle)?,
                },
                members,
                metadata,
                roots,
                missing_sidecars,
            };
            let mut payload = payload;
            payload
                .members
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
            payload
                .metadata
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
            payload
                .roots
                .sort_unstable_by(|left, right| left.path.cmp(&right.path));
            payload.missing_sidecars.sort_unstable();
            validate_payload(&payload, limits, budget)?;
            let encoded = encode_package(&payload, limits, budget)?;
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
        limits: SourceUpgradeRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeRecoveryError> {
        with_candidate_storage(budget, |budget| {
            check_encoded(encoded.len(), limits)?;
            // This reservation also covers escaped-string decoding and the preflight's key set.
            // Shape runs on raw input before header or owned RecoveryPayload deserialization.
            let (reservation, shape) =
                package_decode_reservation(encoded.len(), limits.shape, budget)?;
            budget.charge_source_bytes(encoded.len(), 0, product(encoded.len(), 4)?, 0)?;
            let (payload, retained) = with_temporary(reservation, budget, |_| {
                preflight(encoded, shape)?;
                preflight_admission(encoded, limits)?;
                let header: PersistentFileHeader = serde_json::from_slice(encoded)
                    .map_err(|_| SourceUpgradeRecoveryError::Syntax)?;
                PersistentFileContract::canonical_v1(format_kind(KIND)?, engine_version()?)
                    .validate_header(&header)
                    .map_err(SourceUpgradeRecoveryError::Contract)?;
                let envelope: PersistentFileEnvelope<RecoveryPayload> =
                    serde_json::from_slice(encoded)
                        .map_err(|_| SourceUpgradeRecoveryError::Syntax)?;
                let payload = envelope.into_payload();
                let retained = payload_retained_bytes(&payload)?;
                if retained > reservation {
                    return Err(SourceUpgradeRecoveryError::AccountingOverflow);
                }
                Ok((payload, retained))
            })?;
            // Domain validation overlaps the actual stored payload, not the maximum raw-decode
            // reservation. Inner Scene/catalog owners reserve their own narrowed shape separately.
            budget.charge_source_bytes(0, 0, 0, retained)?;
            validate_payload(&payload, limits, budget)?;
            budget.charge_source_bytes(0, 0, encoded.len(), encoded.len())?;
            let encoded = encoded.to_vec();
            budget.charge_source_bytes(0, 0, encoded.len(), 0)?;
            let digest = digest(&encoded);
            Ok(Self {
                payload,
                encoded,
                digest,
            })
        })
    }

    /// The caller supplies the actual bytes read at this exact Pending reference. Missing and
    /// mismatched data refuse before semantic decoding; this grants no Generic Open bypass.
    pub(super) fn decode_pending(
        reference: &ProjectAuthoringRecovery,
        observed_path: &str,
        observed_bytes: Option<&[u8]>,
        limits: SourceUpgradeRecoveryLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeRecoveryError> {
        with_candidate_storage(budget, |budget| {
            if observed_path != reference.recovery_path().as_str() {
                return Err(SourceUpgradeRecoveryError::PendingPathMismatch);
            }
            let encoded = observed_bytes.ok_or(SourceUpgradeRecoveryError::PendingMissing)?;
            check_encoded(encoded.len(), limits)?;
            budget.charge_source_bytes(0, 0, encoded.len(), 0)?;
            if digest(encoded) != reference.recovery_digest() {
                return Err(SourceUpgradeRecoveryError::PendingDigestMismatch);
            }
            let recovery = Self::decode(encoded, limits, budget)?;
            recovery.validate_recovery_path(observed_path, budget)?;
            Ok(recovery)
        })
    }

    /// The stored package cannot occupy, contain, or lie inside another operation-owned path.
    /// Storage publication and Pending decode share this data-coherence rule; native path
    /// resolution and the actual file/parent observations remain the storage owner's duty.
    pub(super) fn validate_recovery_path(
        &self,
        path: &str,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeRecoveryError> {
        validate_path(path, budget)?;
        let paths = || {
            self.payload
                .file_paths()
                .chain(self.roots().iter().map(|root| root.path.as_str()))
                .chain(self.missing_sidecars().iter().map(String::as_str))
        };
        let (count, total, maximum) = paths().try_fold(
            (0_usize, 0_usize, 0_usize),
            |(count, total, maximum), path| {
                Ok::<_, SourceUpgradeRecoveryError>((
                    sum(count, 1)?,
                    sum(total, path.len())?,
                    maximum.max(path.len()),
                ))
            },
        )?;
        let scratch = product(sum(path.len(), maximum)?, 3)?;
        let work = sum(
            product(total, 3)?,
            product(product(path.len(), 3)?, sum(count, 1)?)?,
        )?;
        budget.charge_source_bytes(0, 0, work, 0)?;
        with_temporary(scratch, budget, |_| {
            let path = path.to_lowercase();
            for owned in paths() {
                let owned = owned.to_lowercase();
                if path == owned || below(&path, &owned) || below(&owned, &path) {
                    return Err(SourceUpgradeRecoveryError::PathConflict);
                }
            }
            Ok(())
        })
    }

    pub(super) fn encoded_bytes(&self) -> &[u8] {
        &self.encoded
    }

    /// Shares immutable operation inputs across live review, storage and publication owners.
    /// The encoded/declaration allocation moves once; this admits only the Arc control block.
    /// Refusal returns the original package and leaves its existing storage charged.
    pub(super) fn into_shared(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Arc<Self>, SourceUpgradeRecoveryShareFailure> {
        let retained_before = budget.retained_candidate_bytes;
        if let Err(error) = budget.charge_source_bytes(0, 0, 0, 4 * size_of::<usize>()) {
            let failed_control_bytes = budget
                .retained_candidate_bytes
                .checked_sub(retained_before)
                .expect("sharing only adds Arc control storage");
            budget.retained_candidate_bytes = budget
                .retained_candidate_bytes
                .checked_sub(failed_control_bytes)
                .expect("the failed Arc control reservation remains charged");
            return Err(SourceUpgradeRecoveryShareFailure {
                recovery: self,
                error: SourceUpgradeRecoveryError::Admission(error),
            });
        }
        Ok(Arc::new(self))
    }
    pub(super) const fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.digest
    }
    pub(super) const fn source_universe(&self) -> SourceUniverseId {
        self.payload.source_universe
    }
    pub(super) fn manifest(&self) -> &RecoveryReplacement {
        &self.payload.manifest
    }
    pub(super) fn bundle(&self) -> &RecoveryReplacement {
        &self.payload.bundle
    }
    pub(super) fn members(&self) -> &[RecoveryMember] {
        &self.payload.members
    }
    pub(super) fn metadata(&self) -> &[RecoveryMetadata] {
        &self.payload.metadata
    }
    pub(super) fn roots(&self) -> &[RecoveryRoot] {
        &self.payload.roots
    }
    pub(super) fn missing_sidecars(&self) -> &[String] {
        &self.payload.missing_sidecars
    }
    pub(super) fn field_upgrade(
        &self,
    ) -> (&ComponentTypeId, &ComponentFieldId, &ComponentPropertyPath) {
        (
            &self.payload.field.component,
            &self.payload.field.field,
            &self.payload.field.successor_property,
        )
    }
}

impl RecoveryPayload {
    fn file_paths(&self) -> impl Iterator<Item = &str> {
        [&self.manifest.path, &self.bundle.path]
            .into_iter()
            .chain(self.members.iter().map(|member| &member.path))
            .chain(self.metadata.iter().map(|metadata| &metadata.path))
            .map(String::as_str)
    }
}

fn validate_payload(
    payload: &RecoveryPayload,
    limits: SourceUpgradeRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeRecoveryError> {
    check_counts(
        payload.members.len(),
        payload.metadata.len(),
        payload.roots.len(),
        payload.missing_sidecars.len(),
        limits,
    )?;
    let aggregate = aggregate_bytes(payload)?;
    check_aggregate(aggregate, limits)?;
    budget.charge_source_bytes(aggregate, aggregate, product(aggregate, 4)?, 0)?;
    if payload.manifest.path != MANIFEST_PATH {
        return Err(SourceUpgradeRecoveryError::ConfigurationMismatch);
    }
    let paths = sum(payload.file_paths().count(), payload.missing_sidecars.len())?;
    let path_bytes = payload
        .file_paths()
        .chain(payload.missing_sidecars.iter().map(String::as_str))
        .try_fold(0, |total, path| sum(total, path.len()))?;
    let folded_path_bytes = product(path_bytes, 3)?;
    budget.charge_source_bytes(0, 0, folded_path_bytes, 0)?;
    with_temporary(
        sum(folded_path_bytes, product(paths, 192)?)?,
        budget,
        |budget| {
            let mut seen = BTreeSet::new();
            for path in payload
                .file_paths()
                .chain(payload.missing_sidecars.iter().map(String::as_str))
            {
                validate_path(path, budget)?;
                // Match the inventory's Rust lowercase policy, including expanding lowercase
                // mappings. This does not claim equivalence to every native filesystem's rules.
                if !seen.insert(path.to_lowercase()) {
                    return Err(SourceUpgradeRecoveryError::PathConflict);
                }
                budget.charge_open_file(0)?;
            }
            for path in &seen {
                if path
                    .match_indices('/')
                    .any(|(end, _)| seen.contains(&path[..end]))
                {
                    return Err(SourceUpgradeRecoveryError::PathConflict);
                }
            }
            Ok(())
        },
    )?;
    require_sorted(payload.members.iter().map(|member| member.path.as_str()))?;
    require_sorted(
        payload
            .metadata
            .iter()
            .map(|metadata| metadata.path.as_str()),
    )?;
    require_sorted(payload.roots.iter().map(|root| root.path.as_str()))?;
    require_sorted(payload.missing_sidecars.iter().map(String::as_str))?;
    let key_bytes = payload
        .members
        .iter()
        .try_fold(0, |bytes, member| sum(bytes, member.key.retained_bytes()))?;
    with_temporary(
        sum(key_bytes, product(payload.members.len(), 192)?)?,
        budget,
        |_| {
            let mut keys = BTreeSet::new();
            let mut stable = BTreeSet::new();
            for member in &payload.members {
                if !keys.insert(&member.key) {
                    return Err(SourceUpgradeRecoveryError::KeyConflict);
                }
                match member.key.origin() {
                    SourceDocumentOrigin::ProjectLocation { universe, path }
                        if *universe == payload.source_universe && path.as_str() == member.path => {
                    }
                    SourceDocumentOrigin::StableAsset { stable_id }
                        if !stable_id.as_uuid().is_nil() && stable.insert(*stable_id) => {}
                    _ => return Err(SourceUpgradeRecoveryError::KeyConflict),
                }
            }
            Ok(())
        },
    )?;
    for bytes in [
        &payload.manifest.old_disk,
        &payload.manifest.successor,
        &payload.bundle.old_disk,
        &payload.bundle.successor,
    ] {
        bytes.validate()?;
    }
    let manifest_bytes = sum(
        payload.manifest.old_disk.bytes.len(),
        payload.manifest.successor.bytes.len(),
    )?;
    with_owner(manifest_bytes, budget, |budget| {
        let old = parse_ready(payload.manifest.old_disk.bytes())?;
        let next = parse_ready(payload.manifest.successor.bytes())?;
        if old != next
            || old.project.source_universe != payload.source_universe
            || old.authoring.schema_bundle.as_deref() != Some(payload.bundle.path.as_str())
        {
            return Err(SourceUpgradeRecoveryError::ConfigurationMismatch);
        }
        let canonical = next
            .to_canonical_toml_bytes()
            .map_err(|_| SourceUpgradeRecoveryError::ManifestInvalid)?;
        if canonical != payload.manifest.successor.bytes() {
            return Err(SourceUpgradeRecoveryError::NonCanonicalSuccessor);
        }
        validate_configurations(payload, &old, budget)
    })?;
    let bundle_bytes = sum(
        payload.bundle.old_disk.bytes.len(),
        payload.bundle.successor.bytes.len(),
    )?;
    let bundle_limits = AuthoringSchemaBundleLimits::default();
    let (reservation, shape) = bundle_decode_reservation(bundle_bytes, bundle_limits, budget)?;
    budget.charge_source_bytes(0, 0, product(bundle_bytes, 4)?, 0)?;
    with_temporary(reservation, budget, |budget| {
        let narrowed = bundle_limits.with_shape(shape);
        let old = AuthoringSchemaBundle::from_json_bytes_with_limits(
            payload.bundle.old_disk.bytes(),
            narrowed,
        )
        .map_err(|_| SourceUpgradeRecoveryError::BundleInvalid)?;
        let next = AuthoringSchemaBundle::from_json_bytes_with_limits(
            payload.bundle.successor.bytes(),
            narrowed,
        )
        .map_err(|_| SourceUpgradeRecoveryError::BundleInvalid)?;
        validate_field_successor(payload, &old, &next, budget)
    })?;
    for (index, member) in payload.members.iter().enumerate() {
        for (bytes, canonical) in [
            (&member.old_disk, false),
            (&member.reviewed_working, false),
            (&member.successor, true),
        ] {
            bytes.validate()?;
            validate_source(member, bytes.bytes(), canonical, limits.scene, budget).map_err(
                |error| match error {
                    SourceUpgradeRecoveryError::SourceInvalid => {
                        SourceUpgradeRecoveryError::MemberInvalid { index }
                    }
                    other => other,
                },
            )?;
        }
    }
    for metadata in &payload.metadata {
        metadata.observed.validate()?;
    }
    Ok(())
}

fn validate_configurations(
    payload: &RecoveryPayload,
    manifest: &ProjectManifest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeRecoveryError> {
    let root_bytes = payload
        .roots
        .iter()
        .try_fold(0, |total, root| sum(total, root.path.len()))?;
    let folded_root_bytes = product(root_bytes, 3)?;
    budget.charge_source_bytes(0, 0, folded_root_bytes, 0)?;
    with_temporary(
        sum(folded_root_bytes, product(payload.roots.len(), 192)?)?,
        budget,
        |budget| {
            let mut roots = BTreeSet::new();
            for root in &payload.roots {
                validate_path(&root.path, budget)?;
                if !roots.insert(root.path.to_lowercase()) {
                    return Err(SourceUpgradeRecoveryError::RootConflict);
                }
                if let RecoveryRootObservation::Missing { ancestor_path } = &root.observation {
                    if !ancestor_path.is_empty() {
                        validate_path(ancestor_path, budget)?;
                        if !root
                            .path
                            .strip_prefix(ancestor_path)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                        {
                            return Err(SourceUpgradeRecoveryError::RootConflict);
                        }
                    }
                    if payload.file_paths().any(|path| below(path, &root.path)) {
                        return Err(SourceUpgradeRecoveryError::RootConflict);
                    }
                }
            }
            // The same complete manifest drives all profiles. Logical root coverage can be
            // checked here; whether these are actual present/missing directories belongs to capture.
            for profile in std::iter::once(None)
                .chain(manifest.profiles.keys().map(|name| Some(name.as_str())))
            {
                budget.charge_source_bytes(0, 0, sum(root_bytes, payload.metadata.len())?, 0)?;
                let settings = manifest
                    .resolve_profile(profile)
                    .map_err(|_| SourceUpgradeRecoveryError::ConfigurationMismatch)?;
                for path in [
                    &settings.paths.assets,
                    &settings.paths.scenes,
                    &settings.paths.prefabs,
                ] {
                    if !payload.roots.iter().any(|root| root.path == path.as_str()) {
                        return Err(SourceUpgradeRecoveryError::RootConflict);
                    }
                }
            }
            for member in &payload.members {
                let location = matches!(
                    member.key.origin(),
                    SourceDocumentOrigin::ProjectLocation { .. }
                );
                if location
                    != (manifest.project.native_document_identity
                        == NativeDocumentIdentity::ProjectLocation)
                {
                    return Err(SourceUpgradeRecoveryError::KeyConflict);
                }
                if !location
                    && !payload
                        .metadata
                        .iter()
                        .any(|metadata| sidecar_of(&metadata.path, &member.path))
                {
                    return Err(SourceUpgradeRecoveryError::MetadataConflict);
                }
                if !payload
                    .metadata
                    .iter()
                    .any(|metadata| sidecar_of(&metadata.path, &member.path))
                    && !payload
                        .missing_sidecars
                        .iter()
                        .any(|path| sidecar_of(path, &member.path))
                {
                    return Err(SourceUpgradeRecoveryError::MetadataConflict);
                }
            }
            for metadata in &payload.metadata {
                with_owner(metadata.observed.bytes.len(), budget, |_| {
                    let candidate =
                        AssetMetaIdentityCandidate::decode_json_bytes(metadata.observed.bytes())
                            .map_err(|_| SourceUpgradeRecoveryError::MetadataConflict)?;
                    let source_path = metadata
                        .path
                        .strip_suffix(".meta")
                        .ok_or(SourceUpgradeRecoveryError::MetadataConflict)?;
                    let mut matched = None;
                    for profile in std::iter::once(None)
                        .chain(manifest.profiles.keys().map(|name| Some(name.as_str())))
                    {
                        let settings = manifest
                            .resolve_profile(profile)
                            .map_err(|_| SourceUpgradeRecoveryError::ConfigurationMismatch)?;
                        for root in [
                            &settings.paths.assets,
                            &settings.paths.scenes,
                            &settings.paths.prefabs,
                            &settings.paths.scripts,
                        ] {
                            let scope = AssetPathScope::under(
                                AssetSourcePath::new(root.as_str())
                                    .map_err(|_| SourceUpgradeRecoveryError::MetadataConflict)?,
                            );
                            let record = candidate.identity().clone().into_record(&scope);
                            if record.path().as_str() == source_path {
                                matched = Some(record);
                                break;
                            }
                        }
                        if matched.is_some() {
                            break;
                        }
                    }
                    let record = matched.ok_or(SourceUpgradeRecoveryError::MetadataConflict)?;
                    if let Some(member) = payload
                        .members
                        .iter()
                        .find(|member| member.path == source_path)
                    {
                        // Compare metadata with the canonical key. The enclosing Host must still
                        // establish this sidecar's actual location through NativeDocumentSourcePolicy.
                        record
                            .validate_source_document_key(&member.key)
                            .map_err(|_| SourceUpgradeRecoveryError::MetadataConflict)?;
                    } else if matches!(
                        record.source_kind(),
                        AssetSourceKind::Scene | AssetSourceKind::Prefab
                    ) {
                        return Err(SourceUpgradeRecoveryError::MetadataConflict);
                    }
                    Ok(())
                })?;
            }
            for path in &payload.missing_sidecars {
                if !payload
                    .members
                    .iter()
                    .any(|member| sidecar_of(path, &member.path))
                {
                    return Err(SourceUpgradeRecoveryError::MetadataConflict);
                }
            }
            Ok(())
        },
    )
}

fn validate_field_successor(
    payload: &RecoveryPayload,
    old: &AuthoringSchemaBundle,
    next: &AuthoringSchemaBundle,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeRecoveryError> {
    // Fresh data-only evidence is obtained from the actual decoded owners. It is used solely
    // for the existing one-field contract validator and is never persisted or returned.
    let basis = |bundle: &AuthoringSchemaBundle| {
        let adapters: Vec<_> = bundle
            .owners()
            .iter()
            .filter_map(|owner| AuthoringAdapterReceipt::from_schema_owner(owner, &BTreeSet::new()))
            .collect();
        AuthoringSchemaBasis::admit(bundle, &adapters)
            .map_err(|_| SourceUpgradeRecoveryError::LineageInvalid)
    };
    let mut selectors = ComponentPropertyReadBudget::new(
        budget.limits.decode_work(),
        budget.limits.aggregate_decoded_bytes(),
    );
    let result = AuthoredFieldUpgrade::prepare(
        basis(old)?,
        basis(next)?,
        payload.field.component.clone(),
        payload.field.field.clone(),
        payload.field.successor_property.clone(),
        &mut selectors,
    )
    .map_err(|_| SourceUpgradeRecoveryError::LineageInvalid);
    budget.charge_source_bytes(
        0,
        0,
        sum(
            selectors.scanned_entries(),
            selectors.scanned_logical_bytes(),
        )?,
        0,
    )?;
    result.map(|_| ())
}

fn validate_source(
    member: &RecoveryMember,
    encoded: &[u8],
    canonical: bool,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeRecoveryError> {
    let encoding_matches = match member.encoding {
        RecoverySourceEncoding::Json => member.path.ends_with(".json"),
        RecoverySourceEncoding::Ron => member.path.ends_with(".ron"),
    };
    if !encoding_matches {
        return Err(SourceUpgradeRecoveryError::SourceInvalid);
    }
    let reservation = super::project::reserve_source_decode(
        encoded.len(),
        limits.encoded_bytes().get(),
        0,
        limits,
        budget,
    )
    .map_err(|error| match error {
        super::project::AuthoringProjectSourceAdmissionError::Budget(error) => {
            SourceUpgradeRecoveryError::Admission(error)
        }
        _ => SourceUpgradeRecoveryError::AccountingOverflow,
    })?;
    let limits = reservation.limits;
    // The source owner charged this temporary reservation before returning it. Release it on
    // every outcome, retaining the cumulative decode work and any other operation charges.
    let result = (|| {
        let encoding = match member.encoding {
            RecoverySourceEncoding::Json => SceneFileEncoding::Json,
            RecoverySourceEncoding::Ron => SceneFileEncoding::Ron,
        };
        match member.key.kind() {
            SourceDocumentKind::Scene => {
                let candidate = match member.encoding {
                    RecoverySourceEncoding::Json => {
                        SceneDocumentCandidate::decode_json_bytes_with_limits(encoded, limits)
                    }
                    RecoverySourceEncoding::Ron => {
                        SceneDocumentCandidate::decode_ron_bytes_with_limits(encoded, limits)
                    }
                }
                .map_err(|_| SourceUpgradeRecoveryError::SourceInvalid)?;
                let document = candidate
                    .into_composition_source()
                    .map_err(|_| SourceUpgradeRecoveryError::SourceInvalid)?;
                if canonical
                    && !canonical_scene_source_matches(document, encoding, encoded, limits)
                        .map_err(|_| SourceUpgradeRecoveryError::SourceInvalid)?
                {
                    return Err(SourceUpgradeRecoveryError::NonCanonicalSuccessor);
                }
            }
            SourceDocumentKind::Prefab => {
                let candidate = match member.encoding {
                    RecoverySourceEncoding::Json => {
                        PrefabDocumentCandidate::decode_json_bytes_with_limits(encoded, limits)
                    }
                    RecoverySourceEncoding::Ron => {
                        PrefabDocumentCandidate::decode_ron_bytes_with_limits(encoded, limits)
                    }
                }
                .map_err(|_| SourceUpgradeRecoveryError::SourceInvalid)?;
                let document = candidate
                    .into_composition_source()
                    .map_err(|_| SourceUpgradeRecoveryError::SourceInvalid)?;
                if canonical
                    && !canonical_prefab_source_matches(document, encoding, encoded, limits)
                        .map_err(|_| SourceUpgradeRecoveryError::SourceInvalid)?
                {
                    return Err(SourceUpgradeRecoveryError::NonCanonicalSuccessor);
                }
            }
        }
        Ok(())
    })();
    budget
        .reconcile_retained_reservation(reservation.bytes, 0)
        .map_err(|_| SourceUpgradeRecoveryError::AccountingOverflow)?;
    result
}

fn parse_ready(bytes: &[u8]) -> Result<ProjectManifest, SourceUpgradeRecoveryError> {
    let loaded = ProjectManifest::parse_toml_bytes(bytes);
    if loaded.has_errors() {
        return Err(SourceUpgradeRecoveryError::ManifestInvalid);
    }
    let manifest = loaded
        .manifest
        .ok_or(SourceUpgradeRecoveryError::ManifestInvalid)?;
    if manifest.authoring.state != ProjectAuthoringState::Ready {
        return Err(SourceUpgradeRecoveryError::ReadyRequired);
    }
    Ok(manifest)
}

fn encode_package(
    payload: &RecoveryPayload,
    limits: SourceUpgradeRecoveryLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<u8>, SourceUpgradeRecoveryError> {
    // Header strings are fixed and admitted before construction too.
    with_temporary(OWNER_FIXED_BYTES, budget, |budget| {
        budget.charge_source_bytes(0, 0, product(payload_retained_bytes(payload)?, 8)?, 0)?;
        let envelope = envelope(KIND, payload)?;
        let mut counter = CountingWriter {
            written: 0,
            maximum: limits.encoded_bytes.get(),
        };
        serde_json::to_writer_pretty(&mut counter, &envelope)
            .map_err(|_| SourceUpgradeRecoveryError::EncodedBytesExceeded)?;
        let length = counter.written;
        budget.charge_source_bytes(length, 0, product(length, 2)?, length)?;
        let mut writer = PackageWriter {
            bytes: Vec::with_capacity(length),
            maximum: length,
        };
        serde_json::to_writer_pretty(&mut writer, &envelope)
            .map_err(|_| SourceUpgradeRecoveryError::EncodedBytesExceeded)?;
        let (reservation, shape) = package_decode_reservation(length, limits.shape, budget)?;
        budget.charge_source_bytes(0, 0, length, 0)?;
        with_temporary(reservation, budget, |_| preflight(&writer.bytes, shape))?;
        Ok(writer.bytes)
    })
}

fn envelope<T>(
    kind: &str,
    payload: T,
) -> Result<PersistentFileEnvelope<T>, SourceUpgradeRecoveryError> {
    Ok(PersistentFileEnvelope::canonical_v1(
        format_kind(kind)?,
        "0.1.0"
            .parse()
            .map_err(|_| SourceUpgradeRecoveryError::HeaderInvalid)?,
        FormatGenerator::new("nara", engine_version()?)
            .map_err(|_| SourceUpgradeRecoveryError::HeaderInvalid)?,
        payload,
    ))
}

fn format_kind(kind: &str) -> Result<FormatKind, SourceUpgradeRecoveryError> {
    FormatKind::new(kind).map_err(|_| SourceUpgradeRecoveryError::HeaderInvalid)
}

fn engine_version() -> Result<EngineVersion, SourceUpgradeRecoveryError> {
    env!("CARGO_PKG_VERSION")
        .parse()
        .map_err(|_| SourceUpgradeRecoveryError::HeaderInvalid)
}

fn preflight(encoded: &[u8], limits: SerdeShapeLimits) -> Result<(), SourceUpgradeRecoveryError> {
    let mut deserializer = serde_json::Deserializer::from_slice(encoded);
    preflight_serde_shape(&mut deserializer, limits).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => SourceUpgradeRecoveryError::Shape(error),
        SerdeShapePreflightError::Parse(_) => SourceUpgradeRecoveryError::Syntax,
    })?;
    deserializer
        .end()
        .map_err(|_| SourceUpgradeRecoveryError::Syntax)
}

// A second walk retains counts instead of collections. Escaped-string parser scratch remains
// covered by the structural reservation. It does not decode Scene/Prefab/catalog grammar:
// no member Vec or retained source String exists before shape and aggregate admission succeed.
#[derive(Deserialize)]
struct PackageAdmission {
    payload: PayloadAdmission,
}

#[derive(Deserialize)]
struct PayloadAdmission {
    manifest: ReplacementAdmission,
    bundle: ReplacementAdmission,
    #[serde(deserialize_with = "measure_members")]
    members: ArrayMeasure,
    #[serde(deserialize_with = "measure_metadata")]
    metadata: ArrayMeasure,
    #[serde(deserialize_with = "measure_observations")]
    roots: ArrayMeasure,
    #[serde(deserialize_with = "measure_observations")]
    missing_sidecars: ArrayMeasure,
}

#[derive(Deserialize)]
struct ReplacementAdmission {
    old_disk: BytesAdmission,
    successor: BytesAdmission,
}

#[derive(Deserialize)]
struct MemberAdmission {
    old_disk: BytesAdmission,
    reviewed_working: BytesAdmission,
    successor: BytesAdmission,
}

#[derive(Deserialize)]
struct MetadataAdmission {
    observed: BytesAdmission,
}

#[derive(Deserialize)]
struct BytesAdmission {
    #[serde(deserialize_with = "measure_text")]
    bytes: usize,
}

#[derive(Default)]
struct ArrayMeasure {
    items: usize,
    bytes: usize,
}

fn preflight_admission(
    encoded: &[u8],
    limits: SourceUpgradeRecoveryLimits,
) -> Result<(), SourceUpgradeRecoveryError> {
    let admission: PackageAdmission =
        serde_json::from_slice(encoded).map_err(|_| SourceUpgradeRecoveryError::Syntax)?;
    let payload = admission.payload;
    check_counts(
        payload.members.items,
        payload.metadata.items,
        payload.roots.items,
        payload.missing_sidecars.items,
        limits,
    )?;
    let total = [
        payload.manifest.old_disk.bytes,
        payload.manifest.successor.bytes,
        payload.bundle.old_disk.bytes,
        payload.bundle.successor.bytes,
        payload.members.bytes,
        payload.metadata.bytes,
    ]
    .into_iter()
    .try_fold(0, sum)?;
    check_aggregate(total, limits)
}

fn measure_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    struct TextMeasure;
    impl serde::de::Visitor<'_> for TextMeasure {
        type Value = usize;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("stored UTF-8 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<usize, E> {
            Ok(text.len())
        }
    }
    deserializer.deserialize_str(TextMeasure)
}

fn measure_members<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayMeasure, D::Error> {
    measure_array::<D, MemberAdmission>(deserializer, |member| {
        member
            .old_disk
            .bytes
            .checked_add(member.reviewed_working.bytes)?
            .checked_add(member.successor.bytes)
    })
}

fn measure_metadata<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayMeasure, D::Error> {
    measure_array::<D, MetadataAdmission>(deserializer, |metadata| Some(metadata.observed.bytes))
}

fn measure_observations<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArrayMeasure, D::Error> {
    measure_array::<D, serde::de::IgnoredAny>(deserializer, |_| Some(0))
}

fn measure_array<'de, D, T>(
    deserializer: D,
    bytes: fn(&T) -> Option<usize>,
) -> Result<ArrayMeasure, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ArrayVisitor<T> {
        bytes: fn(&T) -> Option<usize>,
    }
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ArrayVisitor<T> {
        type Value = ArrayMeasure;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded recovery array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<ArrayMeasure, A::Error> {
            let mut measured = ArrayMeasure::default();
            while let Some(value) = sequence.next_element::<T>()? {
                measured.items = measured
                    .items
                    .checked_add(1)
                    .ok_or_else(|| serde::de::Error::custom("recovery count overflow"))?;
                measured.bytes = (self.bytes)(&value)
                    .and_then(|bytes| measured.bytes.checked_add(bytes))
                    .ok_or_else(|| serde::de::Error::custom("recovery byte count overflow"))?;
            }
            Ok(measured)
        }
    }
    deserializer.deserialize_seq(ArrayVisitor { bytes })
}

fn check_encoded(
    length: usize,
    limits: SourceUpgradeRecoveryLimits,
) -> Result<(), SourceUpgradeRecoveryError> {
    if length > limits.encoded_bytes.get() {
        return Err(SourceUpgradeRecoveryError::EncodedBytesExceeded);
    }
    Ok(())
}

fn check_counts(
    members: usize,
    metadata: usize,
    roots: usize,
    missing: usize,
    limits: SourceUpgradeRecoveryLimits,
) -> Result<(), SourceUpgradeRecoveryError> {
    if members == 0 || members > limits.sources.get() {
        return Err(SourceUpgradeRecoveryError::SourceCountExceeded);
    }
    if sum(sum(metadata, roots)?, missing)? > limits.observations.get() {
        return Err(SourceUpgradeRecoveryError::ObservationCountExceeded);
    }
    Ok(())
}

fn check_aggregate(
    bytes: usize,
    limits: SourceUpgradeRecoveryLimits,
) -> Result<(), SourceUpgradeRecoveryError> {
    if bytes > limits.aggregate_bytes.get() {
        return Err(SourceUpgradeRecoveryError::AggregateBytesExceeded);
    }
    Ok(())
}

fn input_aggregate_bytes(
    input: &SourceUpgradeRecoveryInput<'_>,
) -> Result<usize, SourceUpgradeRecoveryError> {
    let mut total = 0;
    for bytes in [
        input.old_ready_manifest,
        input.future_ready_manifest,
        input.old_bundle,
        input.successor_bundle,
    ]
    .into_iter()
    .chain(
        input
            .members
            .iter()
            .flat_map(|member| [member.old_disk, member.reviewed_working, member.successor]),
    )
    .chain(
        input
            .metadata
            .iter()
            .map(|metadata| metadata.observed_bytes),
    ) {
        total = sum(total, bytes.len())?;
    }
    Ok(total)
}

fn input_copy_work(
    input: &SourceUpgradeRecoveryInput<'_>,
    aggregate: usize,
) -> Result<usize, SourceUpgradeRecoveryError> {
    // RecoveryBytes::copy traverses each byte for UTF-8 validation, copying and hashing.
    let mut work = product(aggregate, 3)?;
    for bytes in [
        size_of::<RecoveryPayload>(),
        MANIFEST_PATH.len(),
        input.bundle_path.len(),
        input.component.as_str().len(),
        input.field.as_str().len(),
        property_retained_bytes(input.successor_property)?,
    ] {
        work = sum(work, bytes)?;
    }
    let mut member_paths = 0;
    for member in input.members {
        member_paths = sum(member_paths, member.path.len())?;
        work = sum(work, member.key.retained_bytes())?;
    }
    let metadata_paths = input
        .metadata
        .iter()
        .try_fold(0, |total, metadata| sum(total, metadata.path.len()))?;
    let mut root_paths = 0;
    for root in input.roots {
        root_paths = sum(root_paths, root.path.len())?;
        work = sum(work, root.missing_ancestor.map_or(0, str::len))?;
    }
    let missing_paths = input
        .missing_sidecars
        .iter()
        .try_fold(0, |total, path| sum(total, path.len()))?;
    for (count, path_bytes) in [
        (input.members.len(), member_paths),
        (input.metadata.len(), metadata_paths),
        (input.roots.len(), root_paths),
        (input.missing_sidecars.len(), missing_paths),
    ] {
        // One copy plus the collection's finite comparison-work model for ordering paths.
        let comparisons = sum((usize::BITS - count.leading_zeros()) as usize, 1)?;
        work = sum(work, product(path_bytes, comparisons)?)?;
    }
    Ok(work)
}

fn aggregate_bytes(payload: &RecoveryPayload) -> Result<usize, SourceUpgradeRecoveryError> {
    let mut total = 0;
    for bytes in [
        &payload.manifest.old_disk,
        &payload.manifest.successor,
        &payload.bundle.old_disk,
        &payload.bundle.successor,
    ]
    .into_iter()
    .chain(payload.members.iter().flat_map(|member| {
        [
            &member.old_disk,
            &member.reviewed_working,
            &member.successor,
        ]
    }))
    .chain(payload.metadata.iter().map(|metadata| &metadata.observed))
    {
        total = sum(total, bytes.bytes.len())?;
    }
    Ok(total)
}

fn input_retained_bytes(
    input: &SourceUpgradeRecoveryInput<'_>,
) -> Result<usize, SourceUpgradeRecoveryError> {
    let mut bytes = sum(size_of::<RecoveryPayload>(), MANIFEST_PATH.len())?;
    bytes = sum(bytes, input.bundle_path.len())?;
    bytes = sum(bytes, input.component.as_str().len())?;
    bytes = sum(bytes, input.field.as_str().len())?;
    bytes = sum(bytes, property_retained_bytes(input.successor_property)?)?;
    for value in [
        input.old_ready_manifest,
        input.future_ready_manifest,
        input.old_bundle,
        input.successor_bundle,
    ] {
        bytes = sum(bytes, value.len())?;
    }
    for member in input.members {
        bytes = sum(bytes, sum(size_of::<RecoveryMember>(), member.path.len())?)?;
        bytes = sum(bytes, member.key.retained_bytes())?;
        for value in [member.old_disk, member.reviewed_working, member.successor] {
            bytes = sum(bytes, value.len())?;
        }
    }
    for metadata in input.metadata {
        bytes = sum(
            bytes,
            sum(
                size_of::<RecoveryMetadata>(),
                sum(metadata.path.len(), metadata.observed_bytes.len())?,
            )?,
        )?;
    }
    for root in input.roots {
        bytes = sum(
            bytes,
            sum(
                size_of::<RecoveryRoot>(),
                sum(root.path.len(), root.missing_ancestor.map_or(0, str::len))?,
            )?,
        )?;
    }
    for path in input.missing_sidecars {
        bytes = sum(bytes, sum(size_of::<String>(), path.len())?)?;
    }
    Ok(bytes)
}

fn payload_retained_bytes(payload: &RecoveryPayload) -> Result<usize, SourceUpgradeRecoveryError> {
    let mut bytes = sum(size_of::<RecoveryPayload>(), aggregate_bytes(payload)?)?;
    bytes = sum(
        bytes,
        sum(payload.manifest.path.len(), payload.bundle.path.len())?,
    )?;
    bytes = sum(
        bytes,
        sum(
            payload.field.component.as_str().len(),
            payload.field.field.as_str().len(),
        )?,
    )?;
    bytes = sum(
        bytes,
        property_retained_bytes(&payload.field.successor_property)?,
    )?;
    bytes = sum(
        bytes,
        product(
            payload.members.capacity() - payload.members.len(),
            size_of::<RecoveryMember>(),
        )?,
    )?;
    bytes = sum(
        bytes,
        product(
            payload.metadata.capacity() - payload.metadata.len(),
            size_of::<RecoveryMetadata>(),
        )?,
    )?;
    bytes = sum(
        bytes,
        product(
            payload.roots.capacity() - payload.roots.len(),
            size_of::<RecoveryRoot>(),
        )?,
    )?;
    bytes = sum(
        bytes,
        product(
            payload.missing_sidecars.capacity() - payload.missing_sidecars.len(),
            size_of::<String>(),
        )?,
    )?;
    for member in &payload.members {
        bytes = sum(
            bytes,
            sum(
                size_of::<RecoveryMember>(),
                sum(member.path.len(), member.key.retained_bytes())?,
            )?,
        )?;
    }
    for metadata in &payload.metadata {
        bytes = sum(
            bytes,
            sum(size_of::<RecoveryMetadata>(), metadata.path.len())?,
        )?;
    }
    for root in &payload.roots {
        bytes = sum(bytes, sum(size_of::<RecoveryRoot>(), root.path.len())?)?;
        if let RecoveryRootObservation::Missing { ancestor_path } = &root.observation {
            bytes = sum(bytes, ancestor_path.len())?;
        }
    }
    for path in &payload.missing_sidecars {
        bytes = sum(bytes, sum(size_of::<String>(), path.len())?)?;
    }
    Ok(bytes)
}

fn validate_path(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeRecoveryError> {
    if path.contains('\\') || path.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return Err(SourceUpgradeRecoveryError::PathInvalid);
    }
    let checked =
        RelativePath::preflight(path).map_err(|_| SourceUpgradeRecoveryError::PathInvalid)?;
    budget.charge_reference(path.len(), checked.components(), false)?;
    budget.charge_source_bytes(0, 0, path.len(), 0)?;
    Ok(())
}

fn require_sorted<'a>(
    paths: impl Iterator<Item = &'a str>,
) -> Result<(), SourceUpgradeRecoveryError> {
    let mut previous = None;
    for path in paths {
        if previous.is_some_and(|previous| previous >= path) {
            return Err(SourceUpgradeRecoveryError::PathConflict);
        }
        previous = Some(path);
    }
    Ok(())
}

fn below(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn sidecar_of(path: &str, source: &str) -> bool {
    path.strip_suffix(".meta") == Some(source)
}

fn text(bytes: &[u8]) -> Result<&str, SourceUpgradeRecoveryError> {
    std::str::from_utf8(bytes).map_err(|_| SourceUpgradeRecoveryError::Utf8)
}

fn digest(bytes: &[u8]) -> ProjectAuthoringRecoveryDigest {
    let value = ContentDigest::of_bytes(bytes);
    ProjectAuthoringRecoveryDigest::from_parts(value.length(), *value.as_bytes())
}

fn classify(
    old: &RecoveryBytes,
    next: &RecoveryBytes,
    observed: Option<ContentDigest>,
) -> RecoveryMemberState {
    let Some(observed) = observed else {
        return RecoveryMemberState::Missing;
    };
    let observed =
        ProjectAuthoringRecoveryDigest::from_parts(observed.length(), *observed.as_bytes());
    match (observed == old.digest, observed == next.digest) {
        (true, true) => RecoveryMemberState::Unchanged,
        (true, false) => RecoveryMemberState::Old,
        (false, true) => RecoveryMemberState::New,
        (false, false) => RecoveryMemberState::Conflict,
    }
}

fn package_decode_reservation(
    encoded: usize,
    shape: SerdeShapeLimits,
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<(usize, SerdeShapeLimits), SourceUpgradeRecoveryError> {
    // Stored source/catalog text is opaque at this boundary. Charge its bytes, then a recovery
    // record/container slot per admitted structural node, never Scene/catalog slots per byte.
    let record = [
        size_of::<RecoveryPayload>(),
        size_of::<RecoveryMember>(),
        size_of::<RecoveryMetadata>(),
        size_of::<RecoveryRoot>(),
        size_of::<ComponentPropertyStep>(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    adaptive_decode_reservation(encoded, shape, sum(record, 192 + 32 + 64)?, 8, budget)
}

fn bundle_decode_reservation(
    encoded: usize,
    limits: AuthoringSchemaBundleLimits,
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<(usize, SerdeShapeLimits), SourceUpgradeRecoveryError> {
    let catalog_node = [
        size_of::<ComponentSchema>(),
        size_of::<ComponentFieldSchema>(),
        size_of::<ComponentShapeMember>(),
        size_of::<ComponentValue>(),
        192,
    ]
    .into_iter()
    .try_fold(0, sum)?;
    // Both bundle owners and the fresh bases coexist. Each file is constrained to the chosen
    // node ceiling, so charge two files and their wire/catalog/basis copies before decoding.
    adaptive_decode_reservation(
        encoded,
        limits.shape(),
        product(catalog_node, 6)?,
        24,
        budget,
    )
}

fn adaptive_decode_reservation(
    encoded: usize,
    shape: SerdeShapeLimits,
    node_bytes: usize,
    text_copies: usize,
    budget: &AuthoringProjectAdmissionBudget,
) -> Result<(usize, SerdeShapeLimits), SourceUpgradeRecoveryError> {
    let fixed = sum(product(encoded, text_copies)?, OWNER_FIXED_BYTES)?;
    let available = budget
        .limits
        .retained_candidate_bytes()
        .get()
        .saturating_sub(budget.retained_candidate_bytes);
    let mut lower = 0;
    let mut upper = shape.nodes().get().min(encoded.max(1));
    while lower < upper {
        let midpoint = lower + (upper - lower) / 2 + 1;
        if sum(fixed, product(midpoint, node_bytes)?)? <= available {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }
    let nodes = ItemLimit::new(lower).ok_or_else(|| {
        SourceUpgradeRecoveryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: budget.limits.retained_candidate_bytes().get(),
            },
        )
    })?;
    let narrowed = SerdeShapeLimits::new(
        shape.depth(),
        nodes,
        ItemLimit::new(shape.container_items().get().min(nodes.get()))
            .ok_or(SourceUpgradeRecoveryError::AccountingOverflow)?,
        shape.string_bytes(),
        shape.total_string_bytes(),
    );
    Ok((sum(fixed, product(nodes.get(), node_bytes)?)?, narrowed))
}

fn small_document_reservation(encoded: usize) -> Result<usize, SourceUpgradeRecoveryError> {
    // Only ProjectManifest and identity-only metadata use this helper. The existing manifest
    // owner admits at most 4096 nodes per file; at most two manifests coexist here. Metadata's
    // own 256-node ceiling is smaller. Full ProjectManifest slots cover their modeled maps and
    // profile/settings projections without interpreting encoded text as unlimited records.
    sum(
        sum(
            product(encoded, 8)?,
            product(
                encoded.min(2 * 4_096),
                sum(size_of::<ProjectManifest>(), 192)?,
            )?,
        )?,
        OWNER_FIXED_BYTES,
    )
}

fn property_retained_bytes(
    property: &ComponentPropertyPath,
) -> Result<usize, SourceUpgradeRecoveryError> {
    sum(
        property
            .logical_bytes()
            .map_err(|_| SourceUpgradeRecoveryError::LineageInvalid)?,
        product(property.steps.len(), size_of::<ComponentPropertyStep>())?,
    )
}

fn with_owner<T>(
    encoded: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeRecoveryError>,
) -> Result<T, SourceUpgradeRecoveryError> {
    budget.charge_source_bytes(0, 0, product(encoded, 4)?, 0)?;
    with_temporary(small_document_reservation(encoded)?, budget, work)
}

fn with_temporary<T>(
    bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeRecoveryError>,
) -> Result<T, SourceUpgradeRecoveryError> {
    budget.charge_source_bytes(0, 0, 0, bytes)?;
    let result = work(budget);
    budget
        .reconcile_retained_reservation(bytes, 0)
        .map_err(|_| SourceUpgradeRecoveryError::AccountingOverflow)?;
    result
}

/// Only a successful result keeps newly prepared ownership. Refusal drops its local models
/// before releasing storage; all encoded/decode/work observations remain cumulative.
fn with_candidate_storage<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeRecoveryError>,
) -> Result<T, SourceUpgradeRecoveryError> {
    let retained_before = budget.retained_candidate_bytes;
    let result = prepare(budget);
    if result.is_err() {
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(SourceUpgradeRecoveryError::AccountingOverflow)?;
        budget
            .reconcile_retained_reservation(temporary, 0)
            .map_err(|_| SourceUpgradeRecoveryError::AccountingOverflow)?;
    }
    result
}

fn sum(left: usize, right: usize) -> Result<usize, SourceUpgradeRecoveryError> {
    left.checked_add(right)
        .ok_or(SourceUpgradeRecoveryError::AccountingOverflow)
}

fn product(left: usize, right: usize) -> Result<usize, SourceUpgradeRecoveryError> {
    left.checked_mul(right)
        .ok_or(SourceUpgradeRecoveryError::AccountingOverflow)
}

struct CountingWriter {
    written: usize,
    maximum: usize,
}
impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .written
            .checked_add(bytes.len())
            .filter(|length| *length <= self.maximum)
            .ok_or_else(|| io::Error::other("recovery byte limit exceeded"))?;
        self.written = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct PackageWriter {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for PackageWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("recovery byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum SourceUpgradeRecoveryError {
    Admission(AuthoringProjectAdmissionError),
    Shape(SerdeShapeError),
    Contract(PersistentFileContractError),
    EncodedBytesExceeded,
    AggregateBytesExceeded,
    SourceCountExceeded,
    ObservationCountExceeded,
    AccountingOverflow,
    HeaderInvalid,
    Syntax,
    Utf8,
    ManifestInvalid,
    ReadyRequired,
    ConfigurationMismatch,
    BundleInvalid,
    LineageInvalid,
    PathInvalid,
    PathConflict,
    KeyConflict,
    RootConflict,
    MetadataConflict,
    SourceInvalid,
    MemberInvalid { index: usize },
    NonCanonicalSuccessor,
    MemberDigestMismatch,
    PendingMissing,
    PendingPathMismatch,
    PendingDigestMismatch,
}

impl From<AuthoringProjectAdmissionError> for SourceUpgradeRecoveryError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl fmt::Display for SourceUpgradeRecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "authored upgrade recovery data refused: {self:?}"
        )
    }
}

impl std::error::Error for SourceUpgradeRecoveryError {}
