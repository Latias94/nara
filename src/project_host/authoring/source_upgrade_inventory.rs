//! Complete, bounded physical participants for a reviewed authored upgrade.
//!
//! This is a source inventory, not Schema/value admission or publication authority. The caller
//! supplies the complete open set, owns working-copy revisions and revalidates the manifest's
//! readiness/publication state separately. Enumeration resolves the retained manifest, including
//! its default configuration and every declared profile. Recovery uses the actual Pending owner
//! and a freshly read bundle, without reconstructing an ordinary Ready receipt. Neither path
//! substitutes the current Browser page or a startup dependency closure.

use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetDatabaseError, AssetMetaFileLimits, AssetMetaFormatError, AssetMetaIdentityCandidate,
    AssetPathScope, AssetRecord, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
};
use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryEntryObservation, DirectoryEntryRejectedFact,
    DirectoryEnumerationLimits, DirectoryEnumerationSession, FileCapability, FileIdentity,
    FileKind, FsError, PathValidationError, RelativePath,
};
use nara_project::{
    EffectiveProjectSettings, ProjectAuthoringRecoveryDigest, ProjectAuthoringState,
    ProjectManifest,
};
use nara_scene::{SceneFileLimits, SourceDocumentKey, SourceDocumentKind};

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError, AuthoringSchemaReceipt,
    document::AuthoringDocumentSourceFact,
    native_source::{AdmittedNativeDocumentSource, NativeDocumentSourcePolicy},
    project::{
        AuthoringProjectSourceAdmissionError, SourceDirectoryBinding, project_reference_bindings,
    },
    source_upgrade_recovery::{
        RecoveryMemberState, RecoveryRootObservation, RecoverySourceEncoding, SourceUpgradeRecovery,
    },
};

#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
use super::source_upgrade_storage::{ObservedPendingRecoveryStorage, RecoveryStorageError};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

// Additional transient handles may overlap project/manifest/bundle, a directory cursor,
// exact-path traversal and native metadata probes. The budget adds all retained inventory
// handles to this conservative baseline; it is not a per-project constant handle footprint.
const HANDLE_PEAK: usize = 12;
const MAP_NODE_BYTES: usize = 192;
// nara_fs verifies path components sequentially: Windows uses one 64 KiB normalized-name Vec;
// Linux's exact-spelling cursor also fits this ceiling. This is a peak, not bytes per component.
const NATIVE_EXACT_OPEN_SCRATCH_BYTES: usize = 65_536;
// Windows' DIRECTORY_ENUMERATION_BUFFER_BYTES is 4 KiB. The pinned Linux rustix raw Dir grows
// geometrically below 64 KiB; returned entry/name allocations have their own observation reserve.
const NATIVE_CURSOR_SCRATCH_BYTES: usize = if cfg!(windows) { 4_096 } else { 65_536 };

/// One member of the caller's complete open set. Unbound documents cannot be omitted merely
/// because they have no persistence fact; they explicitly refuse the reviewed operation.
#[derive(Clone, Copy)]
pub(super) enum SourceUpgradeOpenSource<'a> {
    Bound(&'a AuthoringDocumentSourceFact),
    Unbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceUpgradeEncoding {
    Json,
    Ron,
}

impl SourceUpgradeEncoding {
    fn for_path(path: &str) -> Result<Self, SourceUpgradeInventoryError> {
        if path.ends_with(".json") {
            Ok(Self::Json)
        } else if path.ends_with(".ron") {
            Ok(Self::Ron)
        } else {
            Err(SourceUpgradeInventoryError::UnsupportedEncoding)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceUpgradeFileFact {
    path: String,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    digest: ContentDigest,
}

impl SourceUpgradeFileFact {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn identity(&self) -> FileIdentity {
        self.identity
    }

    pub(super) const fn parent_identity(&self) -> Option<FileIdentity> {
        self.parent_identity
    }

    pub(super) const fn digest(&self) -> ContentDigest {
        self.digest
    }

    pub(super) fn encoded_bytes(&self) -> u64 {
        self.digest.length()
    }

    fn matches_open(&self, source: &AuthoringDocumentSourceFact) -> bool {
        self.path == source.relative_path()
            && self.identity == source.identity()
            && self.parent_identity == source.parent_identity()
            && self.digest == source.digest()
            && self.digest.length() == source.encoded_bytes() as u64
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceUpgradeParticipant {
    file: SourceUpgradeFileFact,
    native: AdmittedNativeDocumentSource,
    encoding: SourceUpgradeEncoding,
    bytes: Vec<u8>,
}

impl SourceUpgradeParticipant {
    pub(super) const fn file(&self) -> &SourceUpgradeFileFact {
        &self.file
    }

    pub(super) fn kind(&self) -> SourceDocumentKind {
        self.native.key().kind()
    }

    pub(super) fn source_key(&self) -> &SourceDocumentKey {
        self.native.key()
    }

    pub(super) const fn native_source(&self) -> &AdmittedNativeDocumentSource {
        &self.native
    }

    pub(super) const fn encoding(&self) -> SourceUpgradeEncoding {
        self.encoding
    }

    /// Original disk bytes. The upgrade owner separately admits/classifies every working copy.
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RootRoles {
    assets: bool,
    scenes: bool,
    prefabs: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SourceUpgradeRootObservation {
    Available {
        identity: FileIdentity,
    },
    /// The first missing component was opened beneath this exact existing ancestor.
    Missing {
        ancestor_path: String,
        ancestor_identity: FileIdentity,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceUpgradeRoot {
    path: String,
    roles: RootRoles,
    observation: SourceUpgradeRootObservation,
}

impl SourceUpgradeRoot {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn observation(&self) -> &SourceUpgradeRootObservation {
        &self.observation
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceUpgradeMetadata {
    file: SourceUpgradeFileFact,
    record: AssetRecord,
    bytes: Box<[u8]>,
    // Selects an actual resolved configuration whose scope admitted this declaration. It does
    // not participate in canonical identity and is deterministic across the same manifest.
    policy: usize,
}

impl SourceUpgradeMetadata {
    pub(super) const fn file(&self) -> &SourceUpgradeFileFact {
        &self.file
    }

    pub(super) const fn record(&self) -> &AssetRecord {
        &self.record
    }

    /// The exact bounded sidecar bytes moved from the original identity admission.
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug)]
pub(super) struct SourceUpgradeInventory {
    root_identity: FileIdentity,
    manifest_identity: FileIdentity,
    manifest_digest: ContentDigest,
    bundle_identity: FileIdentity,
    bundle_digest: ContentDigest,
    scene_limits: SceneFileLimits,
    participants: BTreeMap<String, SourceUpgradeParticipant>,
    metadata: BTreeMap<String, SourceUpgradeMetadata>,
    roots: BTreeMap<String, SourceUpgradeRoot>,
    directories: BTreeMap<String, FileIdentity>,
    missing_sidecars: BTreeMap<String, FileIdentity>,
    open_sources: BTreeMap<String, AuthoringDocumentSourceFact>,
    custody: Arc<InventoryCustody>,
    // Frozen at construction; later unrelated admissions never participate in settlement.
    retained_bytes: usize,
}

impl PartialEq for SourceUpgradeInventory {
    fn eq(&self, other: &Self) -> bool {
        self.root_identity == other.root_identity
            && self.manifest_identity == other.manifest_identity
            && self.manifest_digest == other.manifest_digest
            && self.bundle_identity == other.bundle_identity
            && self.bundle_digest == other.bundle_digest
            && self.scene_limits == other.scene_limits
            && self.participants == other.participants
            && self.metadata == other.metadata
            && self.roots == other.roots
            && self.directories == other.directories
            && self.missing_sidecars == other.missing_sidecars
            && self.open_sources == other.open_sources
            && self
                .custody
                .directories
                .iter()
                .map(|(path, directory)| (path, directory.identity()))
                .eq(other
                    .custody
                    .directories
                    .iter()
                    .map(|(path, directory)| (path, directory.identity())))
    }
}

impl Eq for SourceUpgradeInventory {}

/// Shares the whole original native observation, including every ancestor and negative
/// observation's existing ancestor. Holding a leaf and its immediate parent alone is insufficient.
/// The original retained handle allowance must survive until every shared owner has dropped.
#[derive(Clone, Debug)]
pub(super) struct SharedSourceUpgradeInventoryCustody {
    inner: Arc<InventoryCustody>,
}

impl SharedSourceUpgradeInventoryCustody {
    /// Accounts the original continuing capability owner under a new capture's allowance.
    /// Sharing does not reopen files or replay the previous operation's performed work.
    pub(super) fn admit_shared(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        let mut admitted = *budget;
        admitted.charge_source_bytes(0, 0, 0, self.inner.retained_bytes)?;
        admitted.retain_open_handles(self.inner.files.len() + self.inner.directories.len())?;
        *budget = admitted;
        Ok(())
    }

    pub(super) fn file(&self, path: &str) -> Option<&FileCapability> {
        self.inner.files.get(path).map(Arc::as_ref)
    }

    pub(super) fn parent(&self, path: &str) -> Option<&DirectoryCapability> {
        let parent = split_parent(path).0;
        if parent.is_empty() {
            Some(&self.inner.root)
        } else {
            self.inner.directories.get(parent)
        }
    }
}

/// Continuous object lifetimes backing every retained native identity. Sharing a capability
/// creates no new native handle; its original retained allowance survives until all owners drop.
#[derive(Debug)]
struct InventoryCustody {
    root: DirectoryCapability,
    files: BTreeMap<String, Arc<FileCapability>>,
    directories: BTreeMap<String, DirectoryCapability>,
    // Sum of the actual storage reservations whose owners survive corpus disposal.
    retained_bytes: usize,
}

impl InventoryCustody {
    fn new(root: DirectoryCapability) -> Self {
        Self {
            root,
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            retained_bytes: 0,
        }
    }

    fn shared_directory(&self, path: &str) -> Option<DirectoryCapability> {
        if path.is_empty() {
            Some(self.root.share())
        } else {
            self.directories.get(path).map(DirectoryCapability::share)
        }
    }

    fn retain_directory(
        &mut self,
        path: &str,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<DirectoryCapability, SourceUpgradeInventoryError> {
        if let Some(directory) = self.shared_directory(path) {
            return Ok(directory);
        }
        // Retain each exact parent before retaining a child. A FileCapability's copied parent
        // identity does not keep that directory alive, and a directory handle does not pin its
        // own parents. Reuse canonical paths instead of opening another retained physical handle.
        for end in path
            .match_indices('/')
            .map(|(index, _)| index)
            .chain(std::iter::once(path.len()))
        {
            let prefix = &path[..end];
            if self.directories.contains_key(prefix) {
                continue;
            }
            let (parent_path, leaf) = split_parent(prefix);
            let parent = self
                .shared_directory(parent_path)
                .ok_or(SourceUpgradeInventoryError::SourceChanged)?;
            let custody_bytes = sum(
                prefix.len(),
                MAP_NODE_BYTES
                    + size_of::<DirectoryCapability>()
                    + size_of::<FileCapability>()
                    + 2 * size_of::<usize>(),
            )?;
            let retained_bytes = sum(self.retained_bytes, custody_bytes)?;
            let directory = with_candidate_storage(budget, |budget| {
                let relative = relative_path(leaf, budget)?;
                // FileCapability includes the native handle, identity and capability policy.
                // Its storage bounds the directory's shared inner plus Arc counters here.
                reserve(budget, custody_bytes)?;
                budget.retain_open_handles(1)?;
                budget.charge_open_file(HANDLE_PEAK)?;
                open_directory_exact(&parent, &relative, budget)
            })?;
            self.retained_bytes = retained_bytes;
            self.directories.insert(prefix.to_owned(), directory);
        }
        self.shared_directory(path)
            .ok_or(SourceUpgradeInventoryError::SourceChanged)
    }
}

impl SourceUpgradeInventory {
    /// `complete_open_sources` is the caller's complete open Scene/Prefab set, including inactive,
    /// dirty and unbound documents. An empty slice explicitly asserts none are open.
    /// The shared operation budget already owns the receipt; this does not reset its allowance.
    pub(super) fn capture(
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[SourceUpgradeOpenSource<'_>],
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        Self::capture_with_mode(
            schema,
            complete_open_sources,
            scene_limits,
            InventoryMode::Ready,
            budget,
        )
    }

    fn capture_with_mode(
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[SourceUpgradeOpenSource<'_>],
        scene_limits: SceneFileLimits,
        mode: InventoryMode<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        let root = schema.project_root();
        let input = InventoryCaptureInput {
            root: &root,
            manifest_bytes: schema.manifest_bytes(),
            manifest_identity: schema.manifest_identity(),
            manifest_digest: schema.manifest_digest(),
            bundle_path: schema.bundle_path().as_str(),
            bundle_bytes: schema.bundle_bytes(),
            bundle_identity: schema.bundle_identity(),
            bundle_digest: schema.bundle_digest(),
            selected_settings: Some(schema.settings()),
        };
        Self::capture_with_input(input, complete_open_sources, scene_limits, mode, budget)
    }

    fn capture_with_input(
        input: InventoryCaptureInput<'_>,
        complete_open_sources: &[SourceUpgradeOpenSource<'_>],
        scene_limits: SceneFileLimits,
        mode: InventoryMode<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            Self::capture_impl(input, complete_open_sources, scene_limits, mode, budget)
        })
    }

    fn capture_impl(
        input: InventoryCaptureInput<'_>,
        complete_open_sources: &[SourceUpgradeOpenSource<'_>],
        scene_limits: SceneFileLimits,
        mode: InventoryMode<'_>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        let retained_before = budget.retained_candidate_bytes;
        reserve(
            budget,
            size_of::<Self>() + size_of::<InventoryCapture<'_>>(),
        )?;
        let mut capture = InventoryCapture {
            input,
            root: input.root.share(),
            mode,
            budget,
            policies: Vec::new(),
            roots: BTreeMap::new(),
            claims: BTreeMap::new(),
            entries: BTreeMap::new(),
            directories: BTreeMap::new(),
            metadata: BTreeMap::new(),
            database: ProjectAssetDatabase::default(),
            spellings: BTreeMap::new(),
            identities: HashMap::new(),
            open_sources: BTreeMap::new(),
            missing_sidecars: BTreeMap::new(),
            custody: InventoryCustody::new(input.root.share()),
        };
        capture
            .custody
            .retain_directory(split_parent(input.bundle_path).0, capture.budget)?;
        capture.resolve_configurations()?;
        capture.include_open_sources(complete_open_sources)?;
        if let InventoryMode::PendingUpgrade(recovery) | InventoryMode::Recovered(recovery) = mode {
            // An originally open native source can live in Scripts outside enumerated catalogue
            // roots. Its stored declaration selects the actual native policy, never a Session or
            // an invented open-source fact. Complete key/body comparison follows enumeration.
            for member in recovery.members() {
                capture.claim(&member.path, member.key.kind())?;
            }
        }
        capture.enumerate()?;
        let entries = std::mem::take(&mut capture.entries);
        for (path, entry) in &entries {
            if entry.kind == FileKind::Regular && path.ends_with(".meta") {
                capture.index_metadata(path, Some(entry.identity))?;
            }
        }
        for (path, entry) in &entries {
            if entry.kind != FileKind::Regular || path.ends_with(".meta") {
                continue;
            }
            if let Some(kind) = capture.file_kind(path)? {
                capture.claim(path, kind)?;
            }
        }
        // Catalogue identity is independent of document suffix and may name files that a
        // suffix-only Browser page would never offer. Copy only after admitting the storage.
        let catalogue_claims = capture
            .database
            .records()
            .filter_map(|record| document_kind(record.source_kind()).map(|kind| (record, kind)));
        let mut additional = Vec::new();
        for (record, kind) in catalogue_claims {
            reserve(
                capture.budget,
                2 * size_of::<(String, SourceDocumentKind)>() + record.path().as_str().len(),
            )?;
            additional.push((record.path().as_str().to_owned(), kind));
        }
        for (path, kind) in additional {
            capture.claim(&path, kind)?;
        }
        let claims = std::mem::take(&mut capture.claims);
        let mut participants = BTreeMap::new();
        let mut origins = BTreeMap::new();
        for (path, kind) in claims {
            let encoding = SourceUpgradeEncoding::for_path(&path)?;
            reserve(capture.budget, path.len() + ".meta".len())?;
            let metadata_path = format!("{path}.meta");
            if !capture.metadata.contains_key(&path) {
                capture.index_optional_sidecar(&path, &metadata_path)?;
            }
            let metadata = capture.metadata.get(&path);
            if metadata
                .is_some_and(|metadata| document_kind(metadata.record.source_kind()) != Some(kind))
            {
                return Err(SourceUpgradeInventoryError::KindConflict);
            }
            let policy = metadata.map_or(0, |metadata| metadata.policy);
            let expected = entries.get(&path).map(|entry| entry.identity);
            let (file, bytes) =
                capture.read_source(&path, expected, scene_limits.encoded_bytes().get())?;
            capture.observe_identity(file.identity, &path)?;
            let metadata = capture.metadata.get(&path);
            // The existing owner alone constructs canonical source keys. In particular,
            // catalogue identity never silently switches a project-location project to stable ID.
            let metadata_scratch = AssetMetaFileLimits::default()
                .encoded_bytes()
                .get()
                .saturating_mul(8);
            reserve(capture.budget, metadata_scratch)?;
            let native_result = with_native_open_scratch(capture.budget, |budget| {
                capture.policies[policy]
                    .native
                    .admit_budgeted(&capture.root, &path, kind, file.identity, 0, 3, budget)
                    .map_err(SourceUpgradeInventoryError::Native)
            });
            let metadata_release = release(capture.budget, metadata_scratch);
            let native = native_result?;
            metadata_release?;
            if let Some(metadata) = metadata {
                metadata
                    .record
                    .validate_source_document_key(native.key())
                    .map_err(|_| SourceUpgradeInventoryError::IdentityConflict)?;
                if let Some(observed) = native.metadata_source()
                    && (observed.project_path().as_str() != metadata.file.path
                        || observed.identity() != metadata.file.identity
                        || observed.parent_identity() != metadata.file.parent_identity
                        || observed.digest() != metadata.file.digest)
                {
                    return Err(SourceUpgradeInventoryError::SourceChanged);
                }
            }
            if let Some(open) = capture.open_sources.get(&path)
                && (!file.matches_open(open)
                    || open.source_key() != native.key()
                    || !same_native_metadata(open.native_source(), &native))
            {
                return Err(SourceUpgradeInventoryError::OpenSourceChanged);
            }
            if origins.contains_key(native.key().origin()) {
                return Err(SourceUpgradeInventoryError::IdentityConflict);
            }
            reserve(
                capture.budget,
                sum(MAP_NODE_BYTES, native.key().retained_bytes())?,
            )?;
            origins.insert(native.key().origin().clone(), ());
            reserve(
                capture.budget,
                MAP_NODE_BYTES + size_of::<SourceUpgradeParticipant>(),
            )?;
            participants.insert(
                path,
                SourceUpgradeParticipant {
                    file,
                    native,
                    encoding,
                    bytes,
                },
            );
        }
        // Reopen all observed names, including Missing roots, before returning a complete result.
        // This is current observation, not a cross-process namespace lock or atomic snapshot.
        capture.revalidate(&participants)?;
        // Admit the one shared owner before moving its maps/handles. No wire body, map node or
        // native handle is copied, and sharing this owner later requires no new allocation.
        let custody_bytes = size_of::<InventoryCustody>() + 2 * size_of::<usize>();
        let custody_retained = sum(capture.custody.retained_bytes, custody_bytes)?;
        reserve(capture.budget, custody_bytes)?;
        capture.custody.retained_bytes = custody_retained;
        let retained_bytes = capture
            .budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
        let custody = Arc::new(capture.custody);
        Ok(Self {
            root_identity: input.root.identity(),
            manifest_identity: input.manifest_identity,
            manifest_digest: input.manifest_digest,
            bundle_identity: input.bundle_identity,
            bundle_digest: input.bundle_digest,
            scene_limits,
            participants,
            metadata: capture.metadata,
            roots: capture.roots,
            directories: capture.directories,
            missing_sidecars: capture.missing_sidecars,
            open_sources: capture.open_sources,
            custody,
            retained_bytes,
        })
    }

    pub(super) fn participants(&self) -> impl ExactSizeIterator<Item = &SourceUpgradeParticipant> {
        self.participants.values()
    }

    pub(super) fn scene_limits(&self) -> SceneFileLimits {
        self.scene_limits
    }

    /// Checks the preparation handoff against the already captured authority and complete open
    /// set. This borrows the original facts; current disk validation remains a separate operation.
    pub(super) fn validate_preparation_inputs(
        &self,
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[SourceUpgradeOpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        budget.charge_source_bytes(
            0,
            0,
            3 * size_of::<FileIdentity>() + 2 * size_of::<ContentDigest>(),
            0,
        )?;
        self.validate_authority(schema)?;
        budget.charge_source_bytes(
            0,
            0,
            product(
                complete_open_sources.len(),
                size_of::<SourceUpgradeOpenSource<'_>>(),
            )?,
            0,
        )?;
        if complete_open_sources
            .iter()
            .any(|source| matches!(source, SourceUpgradeOpenSource::Unbound))
        {
            return Err(SourceUpgradeInventoryError::UnboundOpenSource);
        }
        if self.open_sources.len() != complete_open_sources.len() {
            return Err(SourceUpgradeInventoryError::MembershipChanged);
        }
        // Equal cardinality plus finding every captured path excludes a repeated input replacing
        // an omitted source. Charge each borrowed path probe rather than allocating a second set.
        for (path, captured) in &self.open_sources {
            let mut found = false;
            for source in complete_open_sources {
                let SourceUpgradeOpenSource::Bound(source) = source else {
                    return Err(SourceUpgradeInventoryError::UnboundOpenSource);
                };
                budget.charge_source_bytes(
                    0,
                    0,
                    sum(path.len(), size_of::<SourceUpgradeOpenSource<'_>>())?,
                    0,
                )?;
                if path.as_str() != source.relative_path() {
                    continue;
                }
                let work = captured
                    .checked_retained_bytes()
                    .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
                budget.charge_source_bytes(0, 0, work, 0)?;
                if captured != *source {
                    return Err(SourceUpgradeInventoryError::OpenSourceChanged);
                }
                found = true;
                break;
            }
            if !found {
                return Err(SourceUpgradeInventoryError::MembershipChanged);
            }
        }
        Ok(())
    }

    pub(super) fn metadata(&self) -> impl ExactSizeIterator<Item = &SourceUpgradeMetadata> {
        self.metadata.values()
    }

    pub(super) fn roots(&self) -> impl ExactSizeIterator<Item = &SourceUpgradeRoot> {
        self.roots.values()
    }

    pub(super) fn missing_sidecars(&self) -> impl ExactSizeIterator<Item = &str> {
        self.missing_sidecars.keys().map(String::as_str)
    }

    pub(super) fn shared_custody(&self) -> SharedSourceUpgradeInventoryCustody {
        SharedSourceUpgradeInventoryCustody {
            inner: Arc::clone(&self.custody),
        }
    }

    /// Shares the first actual read's continuously retained leaf. The caller must keep its
    /// original handle allowance until every shared owner has dropped; this opens no new file.
    pub(super) fn shared_file(&self, path: &str) -> Option<Arc<FileCapability>> {
        self.custody.files.get(path).map(Arc::clone)
    }

    /// Shares the actual retained parent, including the already-held project root for a root
    /// leaf. Every non-root parent and its own ancestors were pinned before the leaf was read.
    pub(super) fn shared_parent(&self, path: &str) -> Option<DirectoryCapability> {
        self.custody.shared_directory(split_parent(path).0)
    }

    pub(super) fn retained_handle_count(&self) -> usize {
        self.custody.files.len() + self.custody.directories.len()
    }

    /// Joins a complete ordinary Ready capture to the recovery owner's original disk set.
    /// Working-copy conversion and current name revalidation remain with their owning callers.
    pub(super) fn validate_original_recovery(
        &self,
        schema: &AuthoringSchemaReceipt,
        recovery: &SourceUpgradeRecovery,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        self.validate_authority(schema)?;
        let bytes = sum(
            sum(schema.manifest_bytes().len(), schema.bundle_bytes().len())?,
            schema.bundle_path().as_str().len(),
        )?;
        budget.charge_source_bytes(0, 0, bytes, 0)?;
        if schema.manifest_bytes() != recovery.manifest().old_disk.bytes()
            || schema.bundle_bytes() != recovery.bundle().old_disk.bytes()
            || schema.bundle_path().as_str() != recovery.bundle().path
        {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        self.validate_recovery_members(recovery, RecoveryGeneration::Original, budget)
    }

    fn validate_authority(
        &self,
        schema: &AuthoringSchemaReceipt,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if self.root_identity != schema.root_identity()
            || self.manifest_identity != schema.manifest_identity()
            || self.manifest_digest != schema.manifest_digest()
            || self.bundle_identity != schema.bundle_identity()
            || self.bundle_digest != schema.bundle_digest()
        {
            return Err(SourceUpgradeInventoryError::AuthorityChanged);
        }
        Ok(())
    }

    fn validate_recovery_members(
        &self,
        recovery: &SourceUpgradeRecovery,
        generation: RecoveryGeneration,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if self.participants.len() != recovery.members().len()
            || self.metadata.len() != recovery.metadata().len()
            || self.roots.len() != recovery.roots().len()
            || self.missing_sidecars.len() != recovery.missing_sidecars().len()
        {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        // Charge comparisons before looking at source bodies, complete keys or payload digests.
        // Metadata is indexed by source path, whereas the package sorts sidecar paths. Those
        // orders can differ when one source path prefixes another, so use the existing index.
        let mut work = self.comparison_work()?;
        // Include linear search within std's BTree nodes as well as descent, rather than
        // charging one byte comparison per abstract tree level.
        let lookup_depth = product(
            (usize::BITS - self.metadata.len().leading_zeros()) as usize + 1,
            16,
        )?;
        for metadata in recovery.metadata() {
            work = sum(work, product(metadata.path.len(), lookup_depth)?)?;
        }
        budget.charge_source_bytes(0, 0, work, 0)?;
        for (participant, member) in self.participants().zip(recovery.members()) {
            let encoding = match participant.encoding() {
                SourceUpgradeEncoding::Json => RecoverySourceEncoding::Json,
                SourceUpgradeEncoding::Ron => RecoverySourceEncoding::Ron,
            };
            let bytes_match = match generation {
                RecoveryGeneration::Original => participant.bytes() == member.old_disk.bytes(),
                RecoveryGeneration::Successor => participant.bytes() == member.successor.bytes(),
                RecoveryGeneration::Mixed => {
                    match member.classify(Some(participant.file().digest())) {
                        RecoveryMemberState::Old | RecoveryMemberState::Unchanged => {
                            participant.bytes() == member.old_disk.bytes()
                        }
                        RecoveryMemberState::New => participant.bytes() == member.successor.bytes(),
                        RecoveryMemberState::Missing | RecoveryMemberState::Conflict => false,
                    }
                }
            };
            if participant.file().path() != member.path
                || participant.source_key() != &member.key
                || participant.kind() != member.key.kind()
                || encoding != member.encoding
                || !bytes_match
            {
                return Err(SourceUpgradeInventoryError::RecoveryInventory);
            }
        }
        for proposed in recovery.metadata() {
            let observed = proposed
                .path
                .strip_suffix(".meta")
                .and_then(|source| self.metadata.get(source))
                .ok_or(SourceUpgradeInventoryError::RecoveryInventory)?;
            let digest = proposed.observed.digest();
            if observed.file().path() != proposed.path
                || observed.file().digest()
                    != ContentDigest::from_parts(digest.length(), *digest.as_bytes())
            {
                return Err(SourceUpgradeInventoryError::RecoveryInventory);
            }
        }
        for (observed, proposed) in self.roots().zip(recovery.roots()) {
            let same = match (observed.observation(), &proposed.observation) {
                (
                    SourceUpgradeRootObservation::Available { .. },
                    RecoveryRootObservation::Present,
                ) => true,
                (
                    SourceUpgradeRootObservation::Missing { ancestor_path, .. },
                    RecoveryRootObservation::Missing {
                        ancestor_path: proposed,
                    },
                ) => ancestor_path == proposed,
                _ => false,
            };
            if observed.path() != proposed.path || !same {
                return Err(SourceUpgradeInventoryError::RecoveryInventory);
            }
        }
        if self
            .missing_sidecars()
            .ne(recovery.missing_sidecars().iter().map(String::as_str))
        {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        Ok(())
    }

    fn comparison_work(&self) -> Result<usize, SourceUpgradeInventoryError> {
        let mut bytes = size_of::<Self>();
        for participant in self.participants() {
            bytes = sum(bytes, size_of::<SourceUpgradeParticipant>())?;
            bytes = sum(bytes, participant.file().path().len())?;
            bytes = sum(bytes, participant.source_key().retained_bytes())?;
            bytes = sum(bytes, participant.bytes().len())?;
            bytes = sum(
                bytes,
                participant
                    .native_source()
                    .retained_bytes()
                    .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?,
            )?;
        }
        for metadata in self.metadata() {
            bytes = sum(bytes, size_of::<SourceUpgradeMetadata>())?;
            bytes = sum(bytes, metadata.file().path().len())?;
            bytes = sum(bytes, metadata.bytes().len())?;
            bytes = sum(bytes, metadata.record().path().as_str().len())?;
            if let AssetSourceKind::Other(kind) = metadata.record().source_kind() {
                bytes = sum(bytes, kind.len())?;
            }
        }
        for root in self.roots() {
            bytes = sum(bytes, size_of::<SourceUpgradeRoot>())?;
            bytes = sum(bytes, root.path().len())?;
            if let SourceUpgradeRootObservation::Missing { ancestor_path, .. } = root.observation()
            {
                bytes = sum(bytes, ancestor_path.len())?;
            }
        }
        for path in self.directories.keys().chain(self.missing_sidecars.keys()) {
            bytes = sum(bytes, sum(path.len(), size_of::<FileIdentity>())?)?;
        }
        for path in self.custody.directories.keys() {
            bytes = sum(bytes, sum(path.len(), size_of::<FileIdentity>())?)?;
        }
        for (path, source) in &self.open_sources {
            bytes = sum(bytes, path.len())?;
            bytes = sum(
                bytes,
                source
                    .checked_retained_bytes()
                    .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?,
            )?;
        }
        Ok(bytes)
    }

    /// A fresh bounded enumeration under the same captured manifest. The caller passes its
    /// current complete open set again; opening/closing a participant also invalidates review.
    /// All source bytes/facts, metadata membership, root and directory observations must agree.
    pub(super) fn validate(
        &self,
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[SourceUpgradeOpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        self.validate_authority(schema)?;
        with_temporary_storage(budget, |budget| {
            let current = Self::capture(schema, complete_open_sources, self.scene_limits, budget)?;
            budget.charge_source_bytes(0, 0, self.comparison_work()?, 0)?;
            if self != &current {
                return Err(SourceUpgradeInventoryError::MembershipChanged);
            }
            Ok(())
        })
    }
}

/// This process's complete mixed observation under an actual Pending manifest. The enclosing
/// recovery owner keeps `ObservedPendingRecoveryStorage` alive; this owns the current bundle leaf
/// and all source/metadata/ancestor custody. Matching old/new bytes grants neither publication
/// evidence nor ordinary Ready admission, and never executes the converter again.
#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
pub(super) struct RecoveredSourceUpgradeInventory {
    inventory: SourceUpgradeInventory,
    bundle_file: Arc<FileCapability>,
    bundle_parent: DirectoryCapability,
    operation: ProjectAuthoringRecoveryDigest,
    recovery_identity: FileIdentity,
}

#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
impl RecoveredSourceUpgradeInventory {
    pub(super) fn capture(
        pending: &ObservedPendingRecoveryStorage,
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            let wrapper_bytes = size_of::<Self>() - size_of::<SourceUpgradeInventory>();
            reserve(budget, wrapper_bytes)?;
            pending.validate(budget)?;
            let recovery = pending.recovery();
            let bundle = recovery.bundle();
            let maximum = bundle
                .old_disk
                .bytes()
                .len()
                .max(bundle.successor.bytes().len());
            let before_read = budget.retained_candidate_bytes;
            let (fact, bytes, bundle_file, file_custody_bytes) =
                read_file(pending.root(), &bundle.path, None, maximum, budget)?;
            let read_bytes = budget
                .retained_candidate_bytes
                .checked_sub(before_read)
                .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
            let input = InventoryCaptureInput {
                root: pending.root(),
                manifest_bytes: pending.manifest_bytes(),
                manifest_identity: pending.capability().identity(),
                manifest_digest: pending.digest(),
                bundle_path: &bundle.path,
                bundle_bytes: &bytes,
                bundle_identity: fact.identity,
                bundle_digest: fact.digest,
                selected_settings: None,
            };
            let inventory = SourceUpgradeInventory::capture_with_input(
                input,
                &[],
                scene_limits,
                InventoryMode::Recovered(recovery),
                budget,
            )?;
            inventory.validate_recovery_members(recovery, RecoveryGeneration::Mixed, budget)?;
            let bundle_parent = inventory
                .shared_parent(&bundle.path)
                .ok_or(SourceUpgradeInventoryError::SourceChanged)?;
            if fact.parent_identity != Some(bundle_parent.identity()) {
                return Err(SourceUpgradeInventoryError::SourceChanged);
            }
            // Keep the original read leaf continuously alive while checking its current name,
            // after the complete scan and before transferring observations to publication.
            revalidate_file(pending.root(), &fact, budget)?;
            pending.validate(budget)?;
            drop(bytes);
            drop(fact);
            // Only the actual bundle capability survives this read. Return its temporary body,
            // fact and path storage without touching cumulative IO/work or its live handle.
            release(
                budget,
                read_bytes
                    .checked_sub(file_custody_bytes)
                    .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?,
            )?;
            Ok(Self {
                inventory,
                bundle_file,
                bundle_parent,
                operation: recovery.digest(),
                recovery_identity: pending.recovery_file().identity(),
            })
        })
    }

    pub(super) const fn inventory(&self) -> &SourceUpgradeInventory {
        &self.inventory
    }

    /// The same continuously retained leaf whose actual bytes selected Old/New/Unchanged.
    pub(super) const fn bundle_file(&self) -> &Arc<FileCapability> {
        &self.bundle_file
    }

    pub(super) const fn bundle_digest(&self) -> ContentDigest {
        self.inventory.bundle_digest
    }

    /// Shared from inventory custody; all of this directory's ancestors remain retained there.
    pub(super) const fn bundle_parent(&self) -> &DirectoryCapability {
        &self.bundle_parent
    }

    /// Source/metadata/ancestor custody only. The enclosing owner separately transfers the
    /// actual bundle Arc and Pending storage when discarding this mixed inventory's corpus.
    pub(super) fn shared_custody(&self) -> SharedSourceUpgradeInventoryCustody {
        self.inventory.shared_custody()
    }

    /// Freshly scans the complete set again, including every profile and negative observation.
    /// Old-to-New progress, equal bytes at another leaf, or any membership change requires a new
    /// observation; confirmation cannot silently adopt files changed since this capture.
    pub(super) fn validate(
        &self,
        pending: &ObservedPendingRecoveryStorage,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if self.operation != pending.recovery().digest()
            || self.recovery_identity != pending.recovery_file().identity()
            || self.inventory.root_identity != pending.root().identity()
            || self.inventory.manifest_identity != pending.capability().identity()
            || self.inventory.manifest_digest != pending.digest()
        {
            return Err(SourceUpgradeInventoryError::AuthorityChanged);
        }
        with_temporary_storage(budget, |budget| {
            let current = Self::capture(pending, self.inventory.scene_limits, budget)?;
            budget.charge_source_bytes(0, 0, self.inventory.comparison_work()?, 0)?;
            if self.inventory != current.inventory
                || !self.bundle_file.same_object(&current.bundle_file)?
                || !self.bundle_parent.same_object(&current.bundle_parent)?
            {
                return Err(SourceUpgradeInventoryError::MembershipChanged);
            }
            Ok(())
        })
    }
}

/// A complete successor observation under the real, still-Pending manifest. Construction stays
/// private to this owner; it grants neither ordinary Open nor final Ready publication authority.
/// The enclosing operation separately revalidates its Ready read gate and owns publication lanes.
pub(super) struct VerifiedSourceUpgradeInventory {
    inventory: SourceUpgradeInventory,
    schema: Arc<AuthoringSchemaReceipt>,
    operation: ProjectAuthoringRecoveryDigest,
    retained_bytes: usize,
}

/// Allocation settlement prepared before Ready for one exact immutable inventory. The shared
/// token keeps every native object alive while the discarded corpus is destroyed. Preparing or
/// dropping this record alone never refunds the inventory, custody or any shared Schema/body.
#[derive(Debug)]
pub(super) struct PreparedSourceUpgradeInventoryDisposal {
    custody: SharedSourceUpgradeInventoryCustody,
    discarded_bytes: usize,
    inventory_bytes: usize,
}

impl VerifiedSourceUpgradeInventory {
    pub(super) fn shared_custody(&self) -> SharedSourceUpgradeInventoryCustody {
        self.inventory.shared_custody()
    }

    pub(super) fn capture(
        schema: &Arc<AuthoringSchemaReceipt>,
        recovery: &SourceUpgradeRecovery,
        complete_future_open: &[SourceUpgradeOpenSource<'_>],
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            let wrapper_bytes = size_of::<Self>() - size_of::<SourceUpgradeInventory>();
            reserve(budget, wrapper_bytes)?;
            let inventory = SourceUpgradeInventory::capture_with_mode(
                schema,
                complete_future_open,
                scene_limits,
                InventoryMode::PendingUpgrade(recovery),
                budget,
            )?;
            inventory.validate_recovery_members(recovery, RecoveryGeneration::Successor, budget)?;
            let retained_bytes = sum(wrapper_bytes, inventory.retained_bytes)?;
            Ok(Self {
                inventory,
                schema: Arc::clone(schema),
                operation: recovery.digest(),
                retained_bytes,
            })
        })
    }

    /// Prepares the fixed disposal record while admission can still refuse, before Ready. C is
    /// the custody owner's actual captured reservations; D is the rest of this capture's own
    /// frozen allowance. No later global budget delta or adoption-side cost model is involved.
    pub(super) fn prepare_disposal(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<PreparedSourceUpgradeInventoryDisposal, SourceUpgradeInventoryError> {
        with_candidate_storage(budget, |budget| {
            let discarded = self
                .retained_bytes
                .checked_sub(self.inventory.custody.retained_bytes)
                .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
            let record_bytes = size_of::<PreparedSourceUpgradeInventoryDisposal>();
            let discarded_bytes = sum(discarded, record_bytes)?;
            reserve(budget, record_bytes)?;
            Ok(PreparedSourceUpgradeInventoryDisposal {
                custody: self.inventory.shared_custody(),
                discarded_bytes,
                inventory_bytes: self.retained_bytes,
            })
        })
    }

    /// Consumes this exact inventory and its previously bound disposal record. This performs
    /// no allocation, filesystem access, parsing or admission. The enclosing pre-Ready owner
    /// keeps this pair together; supplying another inventory's record is an internal invariant
    /// violation. All native handles and their C allowance continue through the returned token.
    pub(super) fn into_custody(
        self,
        disposal: PreparedSourceUpgradeInventoryDisposal,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> SharedSourceUpgradeInventoryCustody {
        assert!(
            Arc::ptr_eq(&self.inventory.custody, &disposal.custody.inner),
            "inventory disposal belongs to the exact captured custody"
        );
        assert_eq!(
            self.retained_bytes, disposal.inventory_bytes,
            "inventory disposal binds the original capture allowance"
        );
        let PreparedSourceUpgradeInventoryDisposal {
            custody,
            discarded_bytes,
            inventory_bytes: _,
        } = disposal;
        // Destroy every source body, classification fact, index and open-fact copy before
        // releasing D. The moved shared token still owns all capabilities and their storage.
        drop(self);
        budget.retained_candidate_bytes = budget
            .retained_candidate_bytes
            .checked_sub(discarded_bytes)
            .expect("bound inventory corpus and disposal record remain charged until consumed");
        custody
    }

    pub(super) const fn inventory(&self) -> &SourceUpgradeInventory {
        &self.inventory
    }

    pub(super) fn participants(&self) -> impl ExactSizeIterator<Item = &SourceUpgradeParticipant> {
        self.inventory.participants()
    }

    /// Rechecks the retained future-open membership without copying its source facts. The
    /// enclosing Workspace guard separately binds the complete live open set through adoption.
    pub(super) fn validate_retained(
        &self,
        schema: &Arc<AuthoringSchemaReceipt>,
        recovery: &SourceUpgradeRecovery,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        with_temporary_storage(budget, |budget| {
            let count = self.inventory.open_sources.len();
            let references = product(count, size_of::<SourceUpgradeOpenSource<'_>>())?;
            budget.charge_source_bytes(
                0,
                0,
                references,
                sum(size_of::<Vec<SourceUpgradeOpenSource<'_>>>(), references)?,
            )?;
            let mut sources = Vec::with_capacity(count);
            for source in self.inventory.open_sources.values() {
                sources.push(SourceUpgradeOpenSource::Bound(source));
            }
            let result = self.validate(schema, recovery, &sources, budget);
            drop(sources);
            result
        })
    }

    /// Confirm the same operation, receipt lineage and complete future open set. No retained
    /// collection survives confirmation, including refusal; all performed work stays cumulative.
    pub(super) fn validate(
        &self,
        schema: &Arc<AuthoringSchemaReceipt>,
        recovery: &SourceUpgradeRecovery,
        complete_future_open: &[SourceUpgradeOpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if !Arc::ptr_eq(&self.schema, schema) || self.operation != recovery.digest() {
            return Err(SourceUpgradeInventoryError::AuthorityChanged);
        }
        self.inventory.validate_authority(schema)?;
        with_temporary_storage(budget, |budget| {
            let current = Self::capture(
                schema,
                recovery,
                complete_future_open,
                self.inventory.scene_limits,
                budget,
            )?;
            budget.charge_source_bytes(0, 0, self.inventory.comparison_work()?, 0)?;
            if self.inventory != current.inventory {
                return Err(SourceUpgradeInventoryError::MembershipChanged);
            }
            Ok(())
        })
    }
}

#[derive(Clone, Copy)]
enum RecoveryGeneration {
    Original,
    Successor,
    Mixed,
}

#[derive(Clone, Copy)]
enum InventoryMode<'a> {
    Ready,
    PendingUpgrade(&'a SourceUpgradeRecovery),
    Recovered(&'a SourceUpgradeRecovery),
}

impl InventoryMode<'_> {
    fn validate(
        self,
        input: InventoryCaptureInput<'_>,
        manifest: &mut ProjectManifest,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        let recovery = match self {
            Self::Ready => {
                return if manifest.authoring.state == ProjectAuthoringState::Ready {
                    Ok(())
                } else {
                    Err(SourceUpgradeInventoryError::SourceAuthoringPending)
                };
            }
            Self::PendingUpgrade(recovery) | Self::Recovered(recovery) => recovery,
        };
        let ProjectAuthoringState::Pending(reference) = &manifest.authoring.state else {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        };
        if reference.recovery_digest() != recovery.digest() {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        let work = sum(
            input.bundle_bytes.len(),
            sum(
                input.bundle_path.len(),
                reference.recovery_path().as_str().len(),
            )?,
        )?;
        budget.charge_source_bytes(0, 0, work, 0)?;
        let bundle = recovery.bundle();
        let matches_bundle = match self {
            Self::Recovered(_) => match bundle.classify(Some(input.bundle_digest)) {
                RecoveryMemberState::Old | RecoveryMemberState::Unchanged => {
                    input.bundle_bytes == bundle.old_disk.bytes()
                }
                RecoveryMemberState::New => input.bundle_bytes == bundle.successor.bytes(),
                RecoveryMemberState::Missing | RecoveryMemberState::Conflict => false,
            },
            Self::PendingUpgrade(_) => {
                let expected = bundle.successor.digest();
                input.bundle_digest
                    == ContentDigest::from_parts(expected.length(), *expected.as_bytes())
                    && input.bundle_bytes == bundle.successor.bytes()
            }
            Self::Ready => false,
        };
        if input.bundle_path != bundle.path
            || !matches_bundle
            || manifest.authoring.schema_bundle.as_deref() != Some(recovery.bundle().path.as_str())
            || manifest.project.source_universe != recovery.source_universe()
            || input.selected_settings.is_some_and(|settings| {
                settings.project.source_universe != recovery.source_universe()
                    || settings.authoring_state != manifest.authoring.state
            })
        {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        validate_recovery_configuration(
            input.manifest_bytes.len(),
            manifest,
            recovery.manifest().old_disk.bytes(),
            budget,
        )?;
        if matches!(self, Self::Recovered(_)) {
            // Each temporary model drops and returns its storage before parsing the next one.
            // No Ready receipt or ordinary admission is manufactured from either definition.
            validate_recovery_configuration(
                input.manifest_bytes.len(),
                manifest,
                recovery.manifest().successor.bytes(),
                budget,
            )?;
        }
        Ok(())
    }
}

fn validate_recovery_configuration(
    current_bytes: usize,
    manifest: &mut ProjectManifest,
    expected_bytes: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeInventoryError> {
    with_temporary_storage(budget, |budget| {
        let expected = parse_recovery_manifest(expected_bytes, budget)?;
        budget.charge_source_bytes(0, 0, sum(current_bytes, expected_bytes.len())?, 0)?;
        // Normalize only this temporary parsed value for complete modeled comparison, including
        // every profile. Restore the real Pending state before constructing native policies.
        let pending =
            std::mem::replace(&mut manifest.authoring.state, ProjectAuthoringState::Ready);
        let same =
            expected.authoring.state == ProjectAuthoringState::Ready && *manifest == expected;
        manifest.authoring.state = pending;
        if !same {
            return Err(SourceUpgradeInventoryError::RecoveryInventory);
        }
        Ok(())
    })
}

fn parse_recovery_manifest(
    bytes: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ProjectManifest, SourceUpgradeInventoryError> {
    // Match the existing manifest owner's 4,096-node ceiling, reserving resolved profile
    // storage before its parser runs. Strings are charged separately from model slots.
    let slot = sum(
        size_of::<nara_project::ProjectProfileOverlay>(),
        size_of::<EffectiveProjectSettings>(),
    )?;
    let retained = sum(
        size_of::<ProjectManifest>(),
        sum(
            product(bytes.len(), 8)?,
            product(bytes.len().min(4_096), slot)?,
        )?,
    )?;
    budget.charge_source_bytes(0, retained, product(bytes.len(), 4)?, retained)?;
    let load = ProjectManifest::parse_toml_bytes(bytes);
    if load.has_errors() {
        return Err(SourceUpgradeInventoryError::ManifestInvalid);
    }
    load.manifest
        .ok_or(SourceUpgradeInventoryError::ManifestInvalid)
}

struct ConfigurationPolicy {
    native: NativeDocumentSourcePolicy,
    native_scopes: [String; 4],
    catalogue_bindings: Vec<SourceDirectoryBinding>,
}

#[derive(Clone, Copy)]
struct Entry {
    kind: FileKind,
    identity: FileIdentity,
}

/// Borrowed observations, not an alternate admission or a provider abstraction. Ordinary and
/// Verified callers borrow their real receipt; recovery borrows actual Pending storage and its
/// first bounded bundle read. Native policy construction always uses the decoded manifest below.
#[derive(Clone, Copy)]
struct InventoryCaptureInput<'a> {
    root: &'a DirectoryCapability,
    manifest_bytes: &'a [u8],
    manifest_identity: FileIdentity,
    manifest_digest: ContentDigest,
    bundle_path: &'a str,
    bundle_bytes: &'a [u8],
    bundle_identity: FileIdentity,
    bundle_digest: ContentDigest,
    selected_settings: Option<&'a EffectiveProjectSettings>,
}

struct InventoryCapture<'a> {
    input: InventoryCaptureInput<'a>,
    root: DirectoryCapability,
    mode: InventoryMode<'a>,
    budget: &'a mut AuthoringProjectAdmissionBudget,
    policies: Vec<ConfigurationPolicy>,
    roots: BTreeMap<String, SourceUpgradeRoot>,
    claims: BTreeMap<String, SourceDocumentKind>,
    entries: BTreeMap<String, Entry>,
    directories: BTreeMap<String, FileIdentity>,
    metadata: BTreeMap<String, SourceUpgradeMetadata>,
    database: ProjectAssetDatabase,
    spellings: BTreeMap<String, String>,
    identities: HashMap<FileIdentity, String>,
    open_sources: BTreeMap<String, AuthoringDocumentSourceFact>,
    missing_sidecars: BTreeMap<String, FileIdentity>,
    custody: InventoryCustody,
}

impl InventoryCapture<'_> {
    fn read_source(
        &mut self,
        path: &str,
        expected: Option<FileIdentity>,
        maximum: usize,
    ) -> Result<(SourceUpgradeFileFact, Vec<u8>), SourceUpgradeInventoryError> {
        let parent = self
            .custody
            .retain_directory(split_parent(path).0, self.budget)?;
        let (fact, bytes, capability, file_custody_bytes) =
            read_file(&self.root, path, expected, maximum, self.budget)?;
        if fact.parent_identity != Some(parent.identity()) {
            return Err(SourceUpgradeInventoryError::SourceChanged);
        }
        // Move the same first read handle into custody before any subsequent classification
        // or native metadata lookup. A copied identity is never a substitute for this lifetime.
        let map_bytes = sum(
            MAP_NODE_BYTES + size_of::<Arc<FileCapability>>(),
            path.len(),
        )?;
        let retained_bytes = sum(
            self.custody.retained_bytes,
            sum(file_custody_bytes, map_bytes)?,
        )?;
        reserve(self.budget, map_bytes)?;
        self.custody.retained_bytes = retained_bytes;
        self.custody.files.insert(path.to_owned(), capability);
        Ok((fact, bytes))
    }

    fn observe_root(
        &mut self,
        path: &str,
    ) -> Result<SourceUpgradeRootObservation, SourceUpgradeInventoryError> {
        match self.custody.retain_directory(path, self.budget) {
            Ok(directory) => {
                return Ok(SourceUpgradeRootObservation::Available {
                    identity: directory.identity(),
                });
            }
            Err(SourceUpgradeInventoryError::Host(error)) if is_missing(&error) => {}
            Err(error) => return Err(error),
        }
        // retain_directory kept every successfully opened prefix before the first missing
        // component. The project root is already shared, so even an empty ancestor is live.
        let mut ancestor_end = 0;
        let mut ancestor_identity = self.root.identity();
        for end in path
            .match_indices('/')
            .map(|(index, _)| index)
            .chain(std::iter::once(path.len()))
        {
            let Some(directory) = self.custody.shared_directory(&path[..end]) else {
                break;
            };
            ancestor_end = end;
            ancestor_identity = directory.identity();
        }
        reserve(self.budget, ancestor_end)?;
        Ok(SourceUpgradeRootObservation::Missing {
            ancestor_path: path[..ancestor_end].to_owned(),
            ancestor_identity,
        })
    }

    fn resolve_configurations(&mut self) -> Result<(), SourceUpgradeInventoryError> {
        let bytes = self.input.manifest_bytes;
        // Reserve parser/resolution ownership before calling the existing manifest owner. This
        // conservative model is charged to the operation, not an independent scanner budget.
        let scratch = product(bytes.len(), 128)?;
        reserve(self.budget, scratch)?;
        self.budget
            .charge_source_bytes(0, bytes.len(), bytes.len(), 0)?;
        let load = ProjectManifest::parse_toml_bytes(bytes);
        if load.has_errors() {
            return Err(SourceUpgradeInventoryError::ManifestInvalid);
        }
        let mut manifest = load
            .manifest
            .ok_or(SourceUpgradeInventoryError::ManifestInvalid)?;
        self.mode.validate(self.input, &mut manifest, self.budget)?;
        // BTreeMap profile order is canonical; None is always resolved independently first.
        let profiles =
            std::iter::once(None).chain(manifest.profiles.keys().map(|name| Some(name.as_str())));
        for profile in profiles {
            self.budget
                .charge_reference(profile.map_or(0, str::len), 0, true)?;
            reserve(self.budget, scratch)?;
            self.budget
                .charge_source_bytes(0, bytes.len(), bytes.len(), 0)?;
            let settings = manifest
                .resolve_profile(profile)
                .map_err(|_| SourceUpgradeInventoryError::ProfileInvalid)?;
            self.include_configuration(&settings)?;
            drop(settings);
            release(self.budget, scratch)?;
        }
        drop(manifest);
        release(self.budget, scratch)?;
        Ok(())
    }

    fn include_configuration(
        &mut self,
        settings: &EffectiveProjectSettings,
    ) -> Result<(), SourceUpgradeInventoryError> {
        let paths = [
            &settings.paths.assets,
            &settings.paths.scenes,
            &settings.paths.prefabs,
            &settings.paths.scripts,
        ];
        let path_bytes = paths
            .iter()
            .try_fold(0_usize, |total, path| sum(total, path.as_str().len()))?;
        let policy_bytes = sum(
            2 * size_of::<ConfigurationPolicy>(),
            sum(
                product(path_bytes, 5)?,
                settings.paths.import_cache.as_str().len()
                    + 4 * (size_of::<SourceDirectoryBinding>()
                        + size_of::<AssetPathScope>()
                        + 4 * size_of::<usize>()),
            )?,
        )?;
        reserve(self.budget, policy_bytes)?;
        self.policies.push(ConfigurationPolicy {
            native: NativeDocumentSourcePolicy::new(settings),
            native_scopes: paths.map(|path| path.as_str().to_owned()),
            catalogue_bindings: project_reference_bindings(settings),
        });
        for (index, path) in paths[..3].iter().enumerate() {
            self.budget.charge_reference(path.as_str().len(), 0, true)?;
            self.observe_spelling(path.as_str())?;
            if let Some(root) = self.roots.get_mut(path.as_str()) {
                match index {
                    0 => root.roles.assets = true,
                    1 => root.roles.scenes = true,
                    _ => root.roles.prefabs = true,
                }
                continue;
            }
            reserve(
                self.budget,
                sum(
                    MAP_NODE_BYTES + size_of::<SourceUpgradeRoot>(),
                    product(path.as_str().len(), 2)?,
                )?,
            )?;
            let observation = self.observe_root(path.as_str())?;
            let mut roles = RootRoles::default();
            match index {
                0 => roles.assets = true,
                1 => roles.scenes = true,
                _ => roles.prefabs = true,
            }
            self.roots.insert(
                path.as_str().to_owned(),
                SourceUpgradeRoot {
                    path: path.as_str().to_owned(),
                    roles,
                    observation,
                },
            );
        }
        for scene in settings
            .startup
            .default_scene
            .iter()
            .chain(&settings.content.scenes)
        {
            let length = sum(
                sum(settings.paths.scenes.as_str().len(), 1)?,
                scene.as_str().len(),
            )?;
            reserve(self.budget, length)?;
            let path = format!("{}/{}", settings.paths.scenes.as_str(), scene.as_str());
            self.claim(&path, SourceDocumentKind::Scene)?;
        }
        Ok(())
    }

    fn claim(
        &mut self,
        path: &str,
        kind: SourceDocumentKind,
    ) -> Result<(), SourceUpgradeInventoryError> {
        self.budget.charge_reference(path.len(), 0, true)?;
        self.observe_spelling(path)?;
        if let Some(previous) = self.claims.get(path) {
            return if *previous == kind {
                Ok(())
            } else {
                Err(SourceUpgradeInventoryError::KindConflict)
            };
        }
        reserve(
            self.budget,
            sum(MAP_NODE_BYTES + size_of::<SourceDocumentKind>(), path.len())?,
        )?;
        self.claims.insert(path.to_owned(), kind);
        Ok(())
    }

    fn include_open_sources(
        &mut self,
        sources: &[SourceUpgradeOpenSource<'_>],
    ) -> Result<(), SourceUpgradeInventoryError> {
        for source in sources {
            let SourceUpgradeOpenSource::Bound(source) = source else {
                return Err(SourceUpgradeInventoryError::UnboundOpenSource);
            };
            self.claim(source.relative_path(), source.source_key().kind())?;
            if let Some(previous) = self.open_sources.get(source.relative_path()) {
                if previous != *source {
                    return Err(SourceUpgradeInventoryError::OpenSourceChanged);
                }
                continue;
            }
            let bytes = source
                .checked_retained_bytes()
                .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
            reserve(
                self.budget,
                sum(MAP_NODE_BYTES + source.relative_path().len(), bytes)?,
            )?;
            self.open_sources
                .insert(source.relative_path().to_owned(), (*source).clone());
        }
        Ok(())
    }

    fn enumerate(&mut self) -> Result<(), SourceUpgradeInventoryError> {
        let mut pending = BTreeMap::new();
        for (path, root) in &self.roots {
            if let SourceUpgradeRootObservation::Available { identity } = root.observation {
                reserve(self.budget, sum(MAP_NODE_BYTES, path.len())?)?;
                pending.insert(path.clone(), identity);
            }
        }
        while let Some((path, identity)) = pending.pop_first() {
            if let Some(previous) = self.directories.get(&path) {
                if previous != &identity {
                    return Err(SourceUpgradeInventoryError::SourceChanged);
                }
                continue;
            }
            self.observe_identity(identity, &path)?;
            let depth = RelativePath::preflight(&path)?.components();
            let directory = self.custody.retain_directory(&path, self.budget)?;
            if directory.identity() != identity {
                return Err(SourceUpgradeInventoryError::SourceChanged);
            }
            reserve(self.budget, sum(MAP_NODE_BYTES, path.len())?)?;
            self.directories.insert(path.clone(), identity);
            let name_limit = DirectoryEnumerationLimits::default().max_name_bytes().min(
                self.budget
                    .limits
                    .aggregate_path_bytes()
                    .get()
                    .saturating_sub(self.budget.path_bytes),
            );
            let limits = DirectoryEnumerationLimits::new(
                self.budget.limits.unique_files().get(),
                name_limit,
                1,
            );
            let mut cursor = InventoryDirectoryCursor::begin(&directory, limits, self.budget)?;
            let enumerated = (|| {
                loop {
                    // The filesystem owner allocates a bounded observation before we can borrow its
                    // spelling. Admit that maximum first; reject every isolated/truncated entry.
                    let scratch = sum(
                        product(name_limit, 8)?,
                        size_of::<DirectoryEntryObservation>()
                            + size_of::<DirectoryEntryRejectedFact>(),
                    )?;
                    reserve(self.budget, scratch)?;
                    self.budget.charge_open_file(HANDLE_PEAK)?;
                    let batch = cursor.enumeration.next_batch(1)?;
                    if !batch.rejected_entry_facts().is_empty() {
                        return Err(SourceUpgradeInventoryError::EnumerationIncomplete);
                    }
                    for entry in batch.entries() {
                        let name = entry
                            .name()
                            .to_str()
                            .ok_or(SourceUpgradeInventoryError::EnumerationIncomplete)?;
                        let length = sum(sum(path.len(), 1)?, name.len())?;
                        self.budget.charge_reference(length, depth, false)?;
                        reserve(
                            self.budget,
                            sum(product(length, 3)?, MAP_NODE_BYTES + size_of::<Entry>())?,
                        )?;
                        let child = format!("{path}/{name}");
                        let identity = entry
                            .identity()
                            .ok_or(SourceUpgradeInventoryError::EnumerationIncomplete)?;
                        if entry.kind() == FileKind::Directory {
                            let directory = self.custody.retain_directory(&child, self.budget)?;
                            if directory.identity() != identity {
                                return Err(SourceUpgradeInventoryError::SourceChanged);
                            }
                        }
                        self.observe_spelling(&child)?;
                        self.observe_identity(identity, &child)?;
                        if let Some(previous) = self.entries.get(&child) {
                            if previous.identity != identity || previous.kind != entry.kind() {
                                return Err(SourceUpgradeInventoryError::SourceChanged);
                            }
                        } else {
                            self.entries.insert(
                                child.clone(),
                                Entry {
                                    kind: entry.kind(),
                                    identity,
                                },
                            );
                        }
                        match entry.kind() {
                            FileKind::Directory => {
                                pending.insert(child, identity);
                            }
                            FileKind::Regular => {}
                            _ => return Err(SourceUpgradeInventoryError::EnumerationIncomplete),
                        }
                    }
                    let complete = batch.is_complete();
                    drop(batch);
                    release(self.budget, scratch)?;
                    if complete {
                        break;
                    }
                }
                Ok(())
            })();
            // Drop native cursor storage on every loop outcome before refunding its peak.
            let cursor_release = cursor.finish(self.budget);
            enumerated.and(cursor_release)?;
        }
        Ok(())
    }

    fn index_optional_sidecar(
        &mut self,
        source_path: &str,
        metadata_path: &str,
    ) -> Result<(), SourceUpgradeInventoryError> {
        // A present sidecar's first open is its actual retained read; never probe and close it
        // before reopening the same identity for classification.
        match self.index_metadata(metadata_path, None) {
            Ok(()) => Ok(()),
            Err(SourceUpgradeInventoryError::Host(error)) if is_missing(&error) => {
                let parent = self
                    .custody
                    .retain_directory(split_parent(source_path).0, self.budget)?;
                reserve(self.budget, sum(MAP_NODE_BYTES, metadata_path.len())?)?;
                self.missing_sidecars
                    .insert(metadata_path.to_owned(), parent.identity());
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn index_metadata(
        &mut self,
        path: &str,
        expected: Option<FileIdentity>,
    ) -> Result<(), SourceUpgradeInventoryError> {
        let limits = AssetMetaFileLimits::default();
        let (file, bytes) = self.read_source(path, expected, limits.encoded_bytes().get())?;
        let source_path = path
            .strip_suffix(".meta")
            .ok_or(SourceUpgradeInventoryError::KindConflict)?;
        self.observe_spelling(path)?;
        self.observe_spelling(source_path)?;
        self.observe_identity(file.identity, path)?;
        let scratch = sum(
            product(bytes.len(), 8)?,
            product(limits.shape().nodes().get(), 128)?,
        )?;
        reserve(self.budget, scratch)?;
        // Match project::asset_record_retained_bytes: canonical path expansion is decoded
        // ownership, independently of the sidecar's shorter local alias or encoded settings.
        // Admit the known base before decoding, then any Other-kind text before record creation.
        let decoded_record_base = sum(size_of::<AssetRecord>(), source_path.len())?;
        self.budget
            .charge_source_bytes(0, decoded_record_base, bytes.len(), 0)?;
        let candidate = match AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
            bytes.into_boxed_slice(),
            limits,
        ) {
            Ok(candidate) => candidate,
            Err(source) => {
                return Err(SourceUpgradeInventoryError::MetadataFormat {
                    path: file.path,
                    source,
                });
            }
        };
        if let AssetSourceKind::Other(kind) = candidate.identity().source_kind() {
            self.budget.charge_source_bytes(0, kind.len(), 0, 0)?;
        }
        reserve(self.budget, source_path.len())?;
        let canonical_path = AssetSourcePath::new(source_path)
            .map_err(|_| SourceUpgradeInventoryError::MetadataPathMismatch)?;
        let mut catalogue_source = false;
        let mut catalogue_binding = None;
        'policies: for (index, policy) in self.policies.iter().enumerate() {
            for (binding_index, binding) in policy.catalogue_bindings.iter().enumerate() {
                self.budget.charge_source_bytes(
                    0,
                    0,
                    sum(source_path.len(), binding.directory().as_str().len())?,
                    0,
                )?;
                let local = binding.scope().relative_path(&canonical_path);
                catalogue_source |= local.is_some();
                if local == Some(candidate.identity().path().as_str()) {
                    catalogue_binding = Some((index, binding_index));
                    break 'policies;
                }
            }
        }
        let mut native_binding = None;
        if catalogue_binding.is_none() {
            // Metadata discovered under an actual catalogue root must satisfy that owner.
            // Only an explicit open source or a verified recorded recovery member outside those
            // roots may use NativeSource's Scripts scope. Invalid catalogue rows cannot fall back.
            if catalogue_source
                || (!self.open_sources.contains_key(source_path)
                    && !self.is_recorded_member(source_path)?)
            {
                return Err(SourceUpgradeInventoryError::MetadataPathMismatch);
            }
            'native: for (index, policy) in self.policies.iter().enumerate() {
                for (scope_index, scope) in policy.native_scopes.iter().enumerate() {
                    self.budget.charge_source_bytes(
                        0,
                        0,
                        sum(source_path.len(), scope.len())?,
                        0,
                    )?;
                    if below(source_path, scope) == Some(candidate.identity().path().as_str()) {
                        native_binding = Some((index, scope_index));
                        break 'native;
                    }
                }
            }
            if native_binding.is_none() {
                return Err(SourceUpgradeInventoryError::MetadataPathMismatch);
            }
        }
        let identity_bytes = candidate
            .retained_bytes()
            .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
        let record_bytes = sum(
            product(identity_bytes, 2)?,
            sum(
                product(source_path.len(), 12)?,
                MAP_NODE_BYTES * 4
                    + size_of::<SourceUpgradeMetadata>()
                    + size_of::<AssetRecord>() * 3,
            )?,
        )?;
        reserve(self.budget, record_bytes)?;
        // The original bounded read is already retained. Move that same encoded allocation
        // into the inventory so recovery never reopens or reconstructs a sidecar's settings.
        let (identity, bytes) = candidate.into_parts();
        let (policy, record) = if let Some((policy, binding)) = catalogue_binding {
            let scope = self.policies[policy].catalogue_bindings[binding].scope();
            (policy, identity.into_record(scope))
        } else {
            let (policy, scope_index) =
                native_binding.ok_or(SourceUpgradeInventoryError::MetadataPathMismatch)?;
            let prefix =
                AssetSourcePath::new(self.policies[policy].native_scopes[scope_index].as_str())
                    .map_err(|_| SourceUpgradeInventoryError::MetadataPathMismatch)?;
            let scope = AssetPathScope::under(prefix);
            (policy, identity.into_record(&scope))
        };
        if record.path() != &canonical_path {
            return Err(SourceUpgradeInventoryError::MetadataPathMismatch);
        }
        // All metadata participates in global identity admission, including non-document records.
        // Reuse the catalogue's database owner for stable ID, canonical path and kind conflicts.
        self.database.insert(record.clone())?;
        self.metadata.insert(
            source_path.to_owned(),
            SourceUpgradeMetadata {
                file,
                record,
                bytes,
                policy,
            },
        );
        release(self.budget, scratch)?;
        Ok(())
    }

    fn is_recorded_member(&mut self, path: &str) -> Result<bool, SourceUpgradeInventoryError> {
        let recovery = match self.mode {
            InventoryMode::PendingUpgrade(recovery) | InventoryMode::Recovered(recovery) => {
                recovery
            }
            InventoryMode::Ready => return Ok(false),
        };
        let members = recovery.members();
        // The recovery owner already proves this list sorted and unique. Charge the bounded
        // borrowed search rather than cloning another index or rescanning every member per file.
        let depth = (usize::BITS - members.len().leading_zeros()) as usize + 1;
        self.budget
            .charge_source_bytes(0, 0, product(path.len(), depth)?, 0)?;
        Ok(members
            .binary_search_by(|member| member.path.as_str().cmp(path))
            .is_ok())
    }

    fn file_kind(
        &mut self,
        path: &str,
    ) -> Result<Option<SourceDocumentKind>, SourceUpgradeInventoryError> {
        let named = if path.ends_with(".scene.json") || path.ends_with(".scene.ron") {
            Some(SourceDocumentKind::Scene)
        } else if path.ends_with(".prefab.json") || path.ends_with(".prefab.ron") {
            Some(SourceDocumentKind::Prefab)
        } else {
            None
        };
        let encoded = path.ends_with(".json") || path.ends_with(".ron");
        let mut scenes = false;
        let mut prefabs = false;
        for root in self.roots.values() {
            self.budget
                .charge_source_bytes(0, 0, sum(path.len(), root.path.len())?, 0)?;
            if below(path, &root.path).is_some() {
                scenes |= root.roles.scenes;
                prefabs |= root.roles.prefabs;
            }
        }
        let record = self.metadata.get(path).map(|metadata| &metadata.record);
        if let Some(record) = record {
            let kind = document_kind(record.source_kind());
            if (named.is_some() && named != kind)
                || (encoded && (scenes || prefabs) && kind.is_none())
            {
                return Err(SourceUpgradeInventoryError::KindConflict);
            }
            return Ok(kind);
        }
        if named.is_some() {
            return Ok(named);
        }
        if !encoded {
            return Ok(None);
        }
        match (scenes, prefabs) {
            (true, false) => Ok(Some(SourceDocumentKind::Scene)),
            (false, true) => Ok(Some(SourceDocumentKind::Prefab)),
            (false, false) => Ok(None),
            (true, true) => Err(SourceUpgradeInventoryError::KindConflict),
        }
    }

    fn revalidate(
        &mut self,
        participants: &BTreeMap<String, SourceUpgradeParticipant>,
    ) -> Result<(), SourceUpgradeInventoryError> {
        for root in self.roots.values() {
            if observe_root(&self.root, &root.path, self.budget)? != root.observation {
                return Err(SourceUpgradeInventoryError::SourceChanged);
            }
        }
        // Include exact parents outside enumerated roots and Missing ancestors, rather than
        // checking only directories visited by the source scanner.
        for (path, retained) in &self.custody.directories {
            let relative = relative_path(path, self.budget)?;
            self.budget.admit_handle_peak(HANDLE_PEAK)?;
            let current = open_directory_exact(&self.root, &relative, self.budget)?;
            if !retained.same_object(&current)? {
                return Err(SourceUpgradeInventoryError::SourceChanged);
            }
        }
        for fact in participants
            .values()
            .map(|source| &source.file)
            .chain(self.metadata.values().map(|metadata| &metadata.file))
        {
            revalidate_file(&self.root, fact, self.budget)?;
        }
        for (path, parent) in &self.missing_sidecars {
            let relative = relative_path(path, self.budget)?;
            match open_file_exact(&self.root, &relative, self.budget) {
                Err(SourceUpgradeInventoryError::Host(error)) if is_missing(&error) => {}
                Ok(_) => return Err(SourceUpgradeInventoryError::MembershipChanged),
                Err(error) => return Err(error),
            }
            let source_path = path
                .strip_suffix(".meta")
                .expect("only sidecar paths are retained");
            let relative = relative_path(source_path, self.budget)?;
            let source = open_file_exact(&self.root, &relative, self.budget)?;
            if source.parent_identity() != Some(*parent) {
                return Err(SourceUpgradeInventoryError::SourceChanged);
            }
        }
        Ok(())
    }

    fn observe_spelling(&mut self, path: &str) -> Result<(), SourceUpgradeInventoryError> {
        // Reuse the source index's conservative Unicode lowercase rule. Reserve the maximum
        // expansion before allocating a key, then use an ordered index rather than pair scans.
        let bytes = product(path.len(), 3)?;
        reserve(self.budget, bytes)?;
        self.budget.charge_source_bytes(0, 0, bytes, 0)?;
        let key = path.to_lowercase();
        if let Some(previous) = self.spellings.get(&key) {
            if previous != path {
                return Err(SourceUpgradeInventoryError::PathConflict);
            }
            release(self.budget, bytes)?;
            return Ok(());
        }
        reserve(self.budget, sum(MAP_NODE_BYTES, path.len())?)?;
        self.spellings.insert(key, path.to_owned());
        Ok(())
    }

    fn observe_identity(
        &mut self,
        identity: FileIdentity,
        path: &str,
    ) -> Result<(), SourceUpgradeInventoryError> {
        if let Some(previous) = self.identities.get(&identity) {
            return if previous == path {
                Ok(())
            } else {
                Err(SourceUpgradeInventoryError::IdentityConflict)
            };
        }
        reserve(
            self.budget,
            sum(
                2 * (size_of::<(FileIdentity, String)>() + MAP_NODE_BYTES),
                path.len(),
            )?,
        )?;
        self.identities.insert(identity, path.to_owned());
        Ok(())
    }
}

fn read_file(
    root: &DirectoryCapability,
    path: &str,
    expected: Option<FileIdentity>,
    maximum: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(SourceUpgradeFileFact, Vec<u8>, Arc<FileCapability>, usize), SourceUpgradeInventoryError>
{
    with_candidate_storage(budget, |budget| {
        read_file_retained(root, path, expected, maximum, budget)
    })
}

fn read_file_retained(
    root: &DirectoryCapability,
    path: &str,
    expected: Option<FileIdentity>,
    maximum: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(SourceUpgradeFileFact, Vec<u8>, Arc<FileCapability>, usize), SourceUpgradeInventoryError>
{
    let relative = relative_path(path, budget)?;
    let custody_bytes = size_of::<FileCapability>() + 2 * size_of::<usize>();
    reserve(budget, custody_bytes)?;
    budget.retain_open_handles(1)?;
    budget.charge_open_file(HANDLE_PEAK)?;
    let file = open_file_exact(root, &relative, budget)?;
    if expected.is_some_and(|expected| file.identity() != expected) {
        return Err(SourceUpgradeInventoryError::SourceChanged);
    }
    let (limit, _) = budget.read_limit(maximum)?;
    // Stream once using bounded stack storage, then reserve the exact body and copying work
    // before the filesystem owner allocates. A same-length concurrent rewrite must also hash equal.
    let digest = bounded_digest(&file, limit, budget)?;
    let length = usize::try_from(digest.length())
        .map_err(|_| SourceUpgradeInventoryError::AccountingOverflow)?;
    reserve(
        budget,
        sum(
            product(length, 2)?,
            sum(size_of::<SourceUpgradeFileFact>(), path.len())?,
        )?,
    )?;
    budget.charge_source_bytes(length, 0, length, 0)?;
    let bytes = file.read_to_end_bounded(digest.length())?;
    if ContentDigest::of_bytes(&bytes) != digest {
        return Err(SourceUpgradeInventoryError::SourceChanged);
    }
    Ok((
        SourceUpgradeFileFact {
            path: path.to_owned(),
            identity: file.identity(),
            parent_identity: file.parent_identity(),
            digest,
        },
        bytes,
        Arc::new(file),
        custody_bytes,
    ))
}

fn revalidate_file(
    root: &DirectoryCapability,
    fact: &SourceUpgradeFileFact,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceUpgradeInventoryError> {
    let relative = relative_path(&fact.path, budget)?;
    budget.admit_handle_peak(HANDLE_PEAK)?;
    let file = open_file_exact(root, &relative, budget)?;
    if file.identity() != fact.identity
        || file.parent_identity() != fact.parent_identity
        || bounded_digest(
            &file,
            usize::try_from(fact.digest.length())
                .map_err(|_| SourceUpgradeInventoryError::AccountingOverflow)?,
            budget,
        )? != fact.digest
    {
        return Err(SourceUpgradeInventoryError::SourceChanged);
    }
    Ok(())
}

fn bounded_digest(
    file: &FileCapability,
    maximum: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ContentDigest, SourceUpgradeInventoryError> {
    let remaining = budget
        .limits
        .decode_work()
        .get()
        .saturating_sub(budget.decode_work);
    if remaining == 0 {
        return Err(AuthoringProjectAdmissionError::DecodeWorkExceeded {
            maximum: budget.limits.decode_work().get(),
        }
        .into());
    }
    let digest = file.digest(maximum.min(remaining) as u64)?;
    budget.charge_source_bytes(
        0,
        0,
        usize::try_from(digest.length())
            .map_err(|_| SourceUpgradeInventoryError::AccountingOverflow)?,
        0,
    )?;
    Ok(digest)
}

fn observe_root(
    root: &DirectoryCapability,
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceUpgradeRootObservation, SourceUpgradeInventoryError> {
    let relative = relative_path(path, budget)?;
    budget.charge_open_file(HANDLE_PEAK)?;
    match open_directory_exact(root, &relative, budget) {
        Ok(directory) => {
            return Ok(SourceUpgradeRootObservation::Available {
                identity: directory.identity(),
            });
        }
        Err(SourceUpgradeInventoryError::Host(error)) if is_missing(&error) => {}
        Err(error) => return Err(error),
    }
    // A full-path NotFound alone loses the actual existing ancestor. Probe prefixes with the
    // same capability so a replaced ancestor cannot reproduce the old negative observation.
    let mut ancestor_identity = root.identity();
    let mut ancestor_end = 0;
    for end in path
        .match_indices('/')
        .map(|(index, _)| index)
        .chain(std::iter::once(path.len()))
    {
        let relative = relative_path(&path[..end], budget)?;
        budget.admit_handle_peak(HANDLE_PEAK)?;
        match open_directory_exact(root, &relative, budget) {
            Ok(directory) => {
                ancestor_identity = directory.identity();
                ancestor_end = end;
            }
            Err(SourceUpgradeInventoryError::Host(error)) if is_missing(&error) => {
                reserve(budget, ancestor_end)?;
                return Ok(SourceUpgradeRootObservation::Missing {
                    ancestor_path: path[..ancestor_end].to_owned(),
                    ancestor_identity,
                });
            }
            Err(error) => return Err(error),
        }
    }
    Err(SourceUpgradeInventoryError::SourceChanged)
}

fn relative_path(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<RelativePath, SourceUpgradeInventoryError> {
    let preflight = RelativePath::preflight(path)?;
    budget.charge_reference(path.len(), preflight.components(), false)?;
    reserve(
        budget,
        sum(
            product(path.len(), 4)?,
            product(
                preflight.components(),
                size_of::<nara_fs::RelativeComponent>(),
            )?,
        )?,
    )?;
    budget.charge_source_bytes(0, 0, product(preflight.components(), 65_536)?, 0)?;
    Ok(RelativePath::new(path)?)
}

fn with_native_open_scratch<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    open: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeInventoryError>,
) -> Result<T, SourceUpgradeInventoryError> {
    reserve(budget, NATIVE_EXACT_OPEN_SCRATCH_BYTES)?;
    let result = open(budget);
    let released = release(budget, NATIVE_EXACT_OPEN_SCRATCH_BYTES);
    match result {
        Ok(value) => {
            released?;
            Ok(value)
        }
        // Refund even on failure, without replacing the original filesystem/admission error.
        Err(error) => Err(error),
    }
}

fn open_file_exact(
    root: &DirectoryCapability,
    path: &RelativePath,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<FileCapability, SourceUpgradeInventoryError> {
    budget.admit_handle_peak(HANDLE_PEAK)?;
    with_native_open_scratch(budget, |_| root.open_file_exact(path).map_err(Into::into))
}

fn open_directory_exact(
    root: &DirectoryCapability,
    path: &RelativePath,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<DirectoryCapability, SourceUpgradeInventoryError> {
    budget.admit_handle_peak(HANDLE_PEAK)?;
    with_native_open_scratch(budget, |_| {
        root.open_directory_exact(path).map_err(Into::into)
    })
}

/// Concrete custody for the native enumeration buffer, independent of each returned batch.
struct InventoryDirectoryCursor {
    enumeration: DirectoryEnumerationSession,
}

impl InventoryDirectoryCursor {
    fn begin(
        directory: &DirectoryCapability,
        limits: DirectoryEnumerationLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceUpgradeInventoryError> {
        budget.admit_handle_peak(HANDLE_PEAK)?;
        reserve(budget, NATIVE_CURSOR_SCRATCH_BYTES)?;
        match directory.begin_child_enumeration(limits) {
            Ok(enumeration) => Ok(Self { enumeration }),
            Err(error) => {
                let _ = release(budget, NATIVE_CURSOR_SCRATCH_BYTES);
                Err(error.into())
            }
        }
    }

    fn finish(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError> {
        drop(self);
        release(budget, NATIVE_CURSOR_SCRATCH_BYTES)
    }
}

fn same_native_metadata(
    left: &AdmittedNativeDocumentSource,
    right: &AdmittedNativeDocumentSource,
) -> bool {
    match (left.metadata_source(), right.metadata_source()) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.project_path() == right.project_path()
                && left.identity() == right.identity()
                && left.parent_identity() == right.parent_identity()
                && left.digest() == right.digest()
                && left.encoded_bytes() == right.encoded_bytes()
        }
        _ => false,
    }
}

fn document_kind(kind: &AssetSourceKind) -> Option<SourceDocumentKind> {
    match kind {
        AssetSourceKind::Scene => Some(SourceDocumentKind::Scene),
        AssetSourceKind::Prefab => Some(SourceDocumentKind::Prefab),
        _ => None,
    }
}

fn below<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    path.strip_prefix(root)
        .and_then(|tail| tail.strip_prefix('/'))
}

fn split_parent(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn is_missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

fn reserve(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: usize,
) -> Result<(), SourceUpgradeInventoryError> {
    budget.charge_source_bytes(0, 0, 0, bytes)?;
    Ok(())
}

fn with_candidate_storage<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceUpgradeInventoryError>,
) -> Result<T, SourceUpgradeInventoryError> {
    let retained = budget.retained_candidate_bytes;
    let handles = budget.retained_open_handles;
    let result = prepare(budget);
    if result.is_err() {
        // All candidate locals have dropped before this point. Never undo cumulative work or
        // file/shape admission when refunding storage from a failed preparation.
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(retained)
            .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
        release(budget, temporary)?;
        let closed = budget
            .retained_open_handles
            .checked_sub(handles)
            .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
        budget.release_open_handles(closed);
    }
    result
}

fn with_temporary_storage(
    budget: &mut AuthoringProjectAdmissionBudget,
    inspect: impl FnOnce(
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceUpgradeInventoryError>,
) -> Result<(), SourceUpgradeInventoryError> {
    let retained = budget.retained_candidate_bytes;
    let handles = budget.retained_open_handles;
    let result = inspect(budget);
    let temporary = budget
        .retained_candidate_bytes
        .checked_sub(retained)
        .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
    release(budget, temporary)?;
    let closed = budget
        .retained_open_handles
        .checked_sub(handles)
        .ok_or(SourceUpgradeInventoryError::AccountingOverflow)?;
    budget.release_open_handles(closed);
    result
}

fn release(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: usize,
) -> Result<(), SourceUpgradeInventoryError> {
    budget.reconcile_retained_reservation(bytes, 0)?;
    Ok(())
}

fn sum(left: usize, right: usize) -> Result<usize, SourceUpgradeInventoryError> {
    left.checked_add(right)
        .ok_or(SourceUpgradeInventoryError::AccountingOverflow)
}

fn product(left: usize, right: usize) -> Result<usize, SourceUpgradeInventoryError> {
    left.checked_mul(right)
        .ok_or(SourceUpgradeInventoryError::AccountingOverflow)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceUpgradeInventoryError {
    #[error("the complete source inventory exceeded its shared admission allowance")]
    Admission(#[source] AuthoringProjectAdmissionError),
    #[error("native source identity admission failed")]
    Native(#[source] AuthoringProjectSourceAdmissionError),
    #[error("an actual inventory filesystem observation failed")]
    Host(#[source] FsError),
    #[error("an inventory project-relative path is invalid")]
    Path(#[source] PathValidationError),
    #[error("an inventory sidecar declaration is invalid")]
    MetadataFormat {
        path: String,
        #[source]
        source: AssetMetaFormatError,
    },
    #[error("complete inventory catalogue identity admission failed")]
    Catalogue(#[source] AssetDatabaseError),
    #[cfg(any(feature = "tooling", feature = "project-tooling", test))]
    #[error("the actual Pending manifest or recovery observation changed")]
    PendingStorage(#[source] RecoveryStorageError),
    #[error("the complete project manifest is invalid")]
    ManifestInvalid,
    #[error("a declared project profile could not be resolved")]
    ProfileInvalid,
    #[error("ordinary source inventory admission requires a Ready project")]
    SourceAuthoringPending,
    #[error("an inventory document has an unsupported encoding")]
    UnsupportedEncoding,
    #[error("a source directory could not be completely enumerated")]
    EnumerationIncomplete,
    #[error("a sidecar's authored alias does not match its actual source scope")]
    MetadataPathMismatch,
    #[error("inventory paths have conflicting spellings")]
    PathConflict,
    #[error("inventory declarations disagree on the document kind")]
    KindConflict,
    #[error("inventory source identities conflict")]
    IdentityConflict,
    #[error("an actual inventory leaf, parent, name or digest changed")]
    SourceChanged,
    #[error("an open document differs from its captured source observation")]
    OpenSourceChanged,
    #[error("the complete open set contains an unbound document")]
    UnboundOpenSource,
    #[error("the inventory belongs to another project or source observation")]
    AuthorityChanged,
    #[error("the complete inventory changed since capture")]
    MembershipChanged,
    #[error("the complete inventory differs from the verified recovery declarations")]
    RecoveryInventory,
    #[error("source inventory accounting overflowed")]
    AccountingOverflow,
}

impl From<AuthoringProjectAdmissionError> for SourceUpgradeInventoryError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl From<AuthoringProjectSourceAdmissionError> for SourceUpgradeInventoryError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self::Native(error)
    }
}

impl From<FsError> for SourceUpgradeInventoryError {
    fn from(error: FsError) -> Self {
        Self::Host(error)
    }
}

impl From<PathValidationError> for SourceUpgradeInventoryError {
    fn from(error: PathValidationError) -> Self {
        Self::Path(error)
    }
}

impl From<AssetDatabaseError> for SourceUpgradeInventoryError {
    fn from(error: AssetDatabaseError) -> Self {
        Self::Catalogue(error)
    }
}

#[cfg(any(feature = "tooling", feature = "project-tooling", test))]
impl From<RecoveryStorageError> for SourceUpgradeInventoryError {
    fn from(error: RecoveryStorageError) -> Self {
        match error {
            RecoveryStorageError::Admission(error) => Self::Admission(error),
            error => Self::PendingStorage(error),
        }
    }
}
