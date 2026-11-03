//! Complete, bounded physical participants for reviewed source-wide authoring operations.
//!
//! This is a source inventory, not Schema/value admission or publication authority. The caller
//! supplies the complete open set, owns working-copy revisions and revalidates the manifest's
//! readiness/publication state separately. Ordinary capture strictly requires Ready. An operation
//! that owns another authority must admit it before supplying physical inputs and explicit claims.
//! Enumeration resolves the actual manifest's default configuration and every declared profile;
//! it never substitutes the current Browser page or a startup dependency closure.

use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetDatabaseError, AssetMetaFileLimits, AssetMetaFormatError, AssetMetaIdentityCandidate,
    AssetPath, AssetPathScope, AssetRecord, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
};
use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryEntryObservation, DirectoryEntryRejectedFact,
    DirectoryEnumerationLimits, DirectoryEnumerationSession, FileCapability, FileIdentity,
    FileKind, FsError, PathValidationError, RelativePath,
};
use nara_project::{EffectiveProjectSettings, ProjectAuthoringState, ProjectManifest};
use nara_scene::{SceneFileLimits, SourceDocumentKey, SourceDocumentKind};

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError, AuthoringSchemaReceipt,
    document::AuthoringDocumentSourceFact,
    native_source::{AdmittedNativeDocumentSource, NativeDocumentSourcePolicy},
    project::{
        AuthoringProjectSourceAdmissionError, SourceDirectoryBinding, project_reference_bindings,
    },
};

#[cfg(all(test, any(windows, target_os = "linux")))]
pub(super) mod test_support;
#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

mod environment;
pub(super) use environment::{ConfigurationPolicy, ConfigurationRecordPolicy};
mod validation;

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
pub(super) enum OpenSource<'a> {
    Bound(&'a AuthoringDocumentSourceFact),
    Unbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceEncoding {
    Json,
    Ron,
}

impl SourceEncoding {
    fn for_path(path: &str) -> Result<Self, SourceInventoryError> {
        if path.ends_with(".json") {
            Ok(Self::Json)
        } else if path.ends_with(".ron") {
            Ok(Self::Ron)
        } else {
            Err(SourceInventoryError::UnsupportedEncoding)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceFileFact {
    path: String,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    digest: ContentDigest,
}

impl SourceFileFact {
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

/// An exact physical leaf, without native source identity or metadata admission. The enclosing
/// operation interprets presence and bytes; a missing leaf is pinned to its existing parent.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SourceFileObservation {
    Present {
        file: SourceFileFact,
        bytes: Vec<u8>,
    },
    Missing {
        parent_identity: FileIdentity,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceParticipant {
    file: SourceFileFact,
    native: AdmittedNativeDocumentSource,
    explicit_native: bool,
    encoding: SourceEncoding,
    bytes: Vec<u8>,
}

impl SourceParticipant {
    pub(super) const fn file(&self) -> &SourceFileFact {
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

    pub(super) const fn explicit_native(&self) -> bool {
        self.explicit_native
    }

    pub(super) const fn encoding(&self) -> SourceEncoding {
        self.encoding
    }

    /// Original disk bytes. The operation separately admits/classifies every working copy.
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
pub(super) enum SourceRootObservation {
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
pub(super) struct SourceRoot {
    path: String,
    roles: RootRoles,
    observation: SourceRootObservation,
}

impl SourceRoot {
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) const fn observation(&self) -> &SourceRootObservation {
        &self.observation
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SourceMetadata {
    file: SourceFileFact,
    record: AssetRecord,
    authored_path: AssetPath,
    bytes: Box<[u8]>,
    // Selects an actual resolved configuration whose scope admitted this declaration. It does
    // not participate in canonical identity and is deterministic across the same manifest.
    policy: usize,
    visibility: Vec<ConfigurationRecordPolicy>,
}

impl SourceMetadata {
    pub(super) const fn file(&self) -> &SourceFileFact {
        &self.file
    }

    pub(super) const fn record(&self) -> &AssetRecord {
        &self.record
    }

    pub(super) const fn authored_path(&self) -> &AssetPath {
        &self.authored_path
    }

    /// The exact bounded sidecar bytes moved from the original identity admission.
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Every actual configuration/scope that admits this declaration, including native-only
    /// open sources. The historical first policy is not a visibility or resolution shortcut.
    pub(super) fn visibility(&self) -> &[ConfigurationRecordPolicy] {
        &self.visibility
    }
}

#[derive(Debug)]
pub(super) struct SourceInventory {
    root_identity: FileIdentity,
    manifest_identity: FileIdentity,
    manifest_digest: ContentDigest,
    bundle_identity: FileIdentity,
    bundle_digest: ContentDigest,
    scene_limits: SceneFileLimits,
    selected_profile: Option<String>,
    configurations: Vec<ConfigurationPolicy>,
    // Global identity admission is distinct from every configuration's resource visibility.
    // Move the already admitted index into the result instead of discarding and rebuilding it.
    identity_catalogue: ProjectAssetDatabase,
    participants: BTreeMap<String, SourceParticipant>,
    metadata: BTreeMap<String, SourceMetadata>,
    file_observations: BTreeMap<String, SourceFileObservation>,
    roots: BTreeMap<String, SourceRoot>,
    directories: BTreeMap<String, FileIdentity>,
    missing_sidecars: BTreeMap<String, FileIdentity>,
    open_sources: BTreeMap<String, AuthoringDocumentSourceFact>,
    custody: Arc<InventoryCustody>,
    // Frozen at construction; later unrelated admissions never participate in settlement.
    retained_bytes: usize,
}

impl PartialEq for SourceInventory {
    fn eq(&self, other: &Self) -> bool {
        self.root_identity == other.root_identity
            && self.manifest_identity == other.manifest_identity
            && self.manifest_digest == other.manifest_digest
            && self.bundle_identity == other.bundle_identity
            && self.bundle_digest == other.bundle_digest
            && self.scene_limits == other.scene_limits
            && self.selected_profile == other.selected_profile
            && self.configurations == other.configurations
            && self.participants == other.participants
            && self.metadata == other.metadata
            && self.file_observations == other.file_observations
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

impl Eq for SourceInventory {}

/// Shares the whole original native observation, including every ancestor and negative
/// observation's existing ancestor. Holding a leaf and its immediate parent alone is insufficient.
/// The original retained handle allowance must survive until every shared owner has dropped.
#[derive(Clone, Debug)]
pub(super) struct SharedSourceInventoryCustody {
    inner: Arc<InventoryCustody>,
}

impl SharedSourceInventoryCustody {
    #[cfg(test)]
    pub(super) fn weak_owner(&self) -> std::sync::Weak<InventoryCustody> {
        Arc::downgrade(&self.inner)
    }

    #[cfg(test)]
    pub(super) fn file_count(&self) -> usize {
        self.inner.files.len()
    }

    #[cfg(test)]
    pub(super) fn has_directory(&self, path: &str) -> bool {
        self.inner.directories.contains_key(path)
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.inner.retained_bytes
    }

    pub(super) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

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
pub(super) struct InventoryCustody {
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
    ) -> Result<DirectoryCapability, SourceInventoryError> {
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
                .ok_or(SourceInventoryError::SourceChanged)?;
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
            .ok_or(SourceInventoryError::SourceChanged)
    }
}

impl SourceInventory {
    /// `complete_open_sources` is the caller's complete open Scene/Prefab set, including inactive,
    /// dirty and unbound documents. An empty slice explicitly asserts none are open.
    /// The shared operation budget already owns the receipt; this does not reset its allowance.
    pub(super) fn capture(
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[OpenSource<'_>],
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceInventoryError> {
        with_candidate_storage(budget, |budget| {
            let manifest = SourceInventoryManifest::parse(schema.manifest_bytes(), budget)?;
            if manifest.value.authoring.state != ProjectAuthoringState::Ready {
                return Err(SourceInventoryError::SourceAuthoringPending);
            }
            let root = schema.project_root();
            let input = SourceInventoryInput {
                root: &root,
                manifest_bytes: schema.manifest_bytes(),
                manifest_identity: schema.manifest_identity(),
                manifest_digest: schema.manifest_digest(),
                bundle_path: schema.bundle_path().as_str(),
                bundle_identity: schema.bundle_identity(),
                bundle_digest: schema.bundle_digest(),
                selected_settings: Some(schema.settings()),
            };
            Self::capture_admitted(
                input,
                manifest,
                complete_open_sources,
                &[],
                &[],
                scene_limits,
                budget,
            )
        })
    }

    /// Scan finite physical inputs after their concrete operation has admitted authority and
    /// configuration. Claims are sorted unique paths, borrowed from that admitted operation.
    /// Their path/kind asserts physical membership; the flag preserves original explicit native
    /// participation. Claims supply no generation/transition meaning or invented open fact, and
    /// all source capability and identity checks still apply.
    /// Observed files are sorted unique exact leaves, retained without promoting them to native
    /// sources or metadata. Required source declarations cannot occupy those leaves or sidecars.
    pub(super) fn capture_admitted(
        input: SourceInventoryInput<'_>,
        manifest: SourceInventoryManifest,
        complete_open_sources: &[OpenSource<'_>],
        claims: &[SourceClaim<'_>],
        observed_files: &[ObservedFileClaim<'_>],
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceInventoryError> {
        // The enclosing owner entered candidate storage before parsing this manifest. Its
        // reservation is consumed below, so a nested baseline taken now could not refund it.
        Self::capture_impl(
            input,
            manifest,
            complete_open_sources,
            claims,
            observed_files,
            scene_limits,
            budget,
        )
    }

    fn capture_impl(
        input: SourceInventoryInput<'_>,
        manifest: SourceInventoryManifest,
        complete_open_sources: &[OpenSource<'_>],
        claims: &[SourceClaim<'_>],
        observed_files: &[ObservedFileClaim<'_>],
        scene_limits: SceneFileLimits,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceInventoryError> {
        // The parsed manifest's admitted scratch is consumed and released during configuration
        // resolution. It is not part of the frozen corpus or a later custody settlement.
        let retained_before = budget
            .retained_candidate_bytes
            .checked_sub(manifest.retained_bytes)
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        reserve(budget, size_of::<Self>())?;
        let mut capture = InventoryCapture::new(input, claims, observed_files, budget)?;
        capture
            .custody
            .retain_directory(split_parent(input.bundle_path).0, capture.budget)?;
        capture.resolve_configurations(manifest)?;
        capture.include_open_sources(complete_open_sources)?;
        for claim in claims {
            // Every physical claim remains required. Only explicit native admission can add
            // contexts outside catalogue roots, without inventing open-source facts.
            capture.claim(claim.path, claim.kind)?;
        }
        capture.enumerate()?;
        let entries = std::mem::take(&mut capture.entries);
        capture.observe_files(&entries)?;
        for (path, entry) in &entries {
            if entry.kind == FileKind::Regular
                && path.ends_with(".meta")
                && !capture.is_observed_file(path)?
            {
                capture.index_metadata(path, Some(entry.identity))?;
            }
        }
        for (path, entry) in &entries {
            if entry.kind != FileKind::Regular
                || path.ends_with(".meta")
                || capture.is_observed_file(path)?
            {
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
            let encoding = SourceEncoding::for_path(&path)?;
            reserve(capture.budget, path.len() + ".meta".len())?;
            let metadata_path = format!("{path}.meta");
            if !capture.metadata.contains_key(&path) {
                capture.index_optional_sidecar(&path, &metadata_path)?;
            }
            let metadata = capture.metadata.get(&path);
            if metadata
                .is_some_and(|metadata| document_kind(metadata.record.source_kind()) != Some(kind))
            {
                return Err(SourceInventoryError::KindConflict);
            }
            let expected = entries.get(&path).map(|entry| entry.identity);
            let (file, bytes) =
                capture.read_source(&path, expected, scene_limits.encoded_bytes().get())?;
            capture.observe_identity(file.identity, &path)?;
            // The existing owner alone constructs canonical source keys. In particular,
            // catalogue identity never silently switches a project-location project to stable ID.
            let native = capture.admit_participant_contexts(&path, kind, file.identity)?;
            let explicit_native =
                capture.open_sources.contains_key(&path) || capture.explicit_native_claim(&path)?;
            let metadata = capture.metadata.get(&path);
            if let Some(metadata) = metadata {
                metadata
                    .record
                    .validate_source_document_key(native.key())
                    .map_err(|_| SourceInventoryError::IdentityConflict)?;
                if let Some(observed) = native.metadata_source()
                    && (observed.project_path().as_str() != metadata.file.path
                        || observed.identity() != metadata.file.identity
                        || observed.parent_identity() != metadata.file.parent_identity
                        || observed.digest() != metadata.file.digest)
                {
                    return Err(SourceInventoryError::SourceChanged);
                }
            }
            if let Some(open) = capture.open_sources.get(&path)
                && (!file.matches_open(open)
                    || open.source_key() != native.key()
                    || !same_native_metadata(open.native_source(), &native))
            {
                return Err(SourceInventoryError::OpenSourceChanged);
            }
            if origins.contains_key(native.key().origin()) {
                return Err(SourceInventoryError::IdentityConflict);
            }
            reserve(
                capture.budget,
                sum(MAP_NODE_BYTES, native.key().retained_bytes())?,
            )?;
            origins.insert(native.key().origin().clone(), ());
            reserve(
                capture.budget,
                MAP_NODE_BYTES + size_of::<SourceParticipant>(),
            )?;
            participants.insert(
                path,
                SourceParticipant {
                    file,
                    native,
                    explicit_native,
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
        let selected_profile = input
            .selected_settings
            .and_then(|settings| settings.profile_name.as_deref());
        reserve(capture.budget, selected_profile.map_or(0, str::len))?;
        let retained_bytes = capture
            .budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        let custody = Arc::new(capture.custody);
        Ok(Self {
            root_identity: input.root.identity(),
            manifest_identity: input.manifest_identity,
            manifest_digest: input.manifest_digest,
            bundle_identity: input.bundle_identity,
            bundle_digest: input.bundle_digest,
            scene_limits,
            selected_profile: selected_profile.map(str::to_owned),
            identity_catalogue: capture.database,
            configurations: capture.policies,
            participants,
            metadata: capture.metadata,
            file_observations: capture.file_observations,
            roots: capture.roots,
            directories: capture.directories,
            missing_sidecars: capture.missing_sidecars,
            open_sources: capture.open_sources,
            custody,
            retained_bytes,
        })
    }

    pub(super) fn participants(&self) -> impl ExactSizeIterator<Item = &SourceParticipant> {
        self.participants.values()
    }

    pub(super) fn participant(&self, path: &str) -> Option<&SourceParticipant> {
        self.participants.get(path)
    }

    pub(super) fn file_observation(&self, path: &str) -> Option<&SourceFileObservation> {
        self.file_observations.get(path)
    }

    /// Default first, then every named profile in manifest order, including empty contexts.
    pub(super) fn configurations(&self) -> &[ConfigurationPolicy] {
        &self.configurations
    }

    pub(super) fn selected_profile(&self) -> Option<&str> {
        self.selected_profile.as_deref()
    }

    pub(super) fn scene_limits(&self) -> SceneFileLimits {
        self.scene_limits
    }

    /// Checks the preparation handoff against the already captured authority and complete open
    /// set. This borrows the original facts; current disk validation remains a separate operation.
    pub(super) fn validate_preparation_inputs(
        &self,
        schema: &AuthoringSchemaReceipt,
        complete_open_sources: &[OpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceInventoryError> {
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
            product(complete_open_sources.len(), size_of::<OpenSource<'_>>())?,
            0,
        )?;
        if complete_open_sources
            .iter()
            .any(|source| matches!(source, OpenSource::Unbound))
        {
            return Err(SourceInventoryError::UnboundOpenSource);
        }
        if self.open_sources.len() != complete_open_sources.len() {
            return Err(SourceInventoryError::MembershipChanged);
        }
        // Equal cardinality plus finding every captured path excludes a repeated input replacing
        // an omitted source. Charge each borrowed path probe rather than allocating a second set.
        for (path, captured) in &self.open_sources {
            let mut found = false;
            for source in complete_open_sources {
                let OpenSource::Bound(source) = source else {
                    return Err(SourceInventoryError::UnboundOpenSource);
                };
                budget.charge_source_bytes(
                    0,
                    0,
                    sum(path.len(), size_of::<OpenSource<'_>>())?,
                    0,
                )?;
                if path.as_str() != source.relative_path() {
                    continue;
                }
                let work = captured
                    .checked_retained_bytes()
                    .ok_or(SourceInventoryError::AccountingOverflow)?;
                budget.charge_source_bytes(0, 0, work, 0)?;
                if captured != *source {
                    return Err(SourceInventoryError::OpenSourceChanged);
                }
                found = true;
                break;
            }
            if !found {
                return Err(SourceInventoryError::MembershipChanged);
            }
        }
        Ok(())
    }

    pub(super) fn metadata(&self) -> impl ExactSizeIterator<Item = &SourceMetadata> {
        self.metadata.values()
    }

    pub(super) fn metadata_for_source(&self, path: &str) -> Option<&SourceMetadata> {
        self.metadata.get(path)
    }

    /// Global proposed identity admission, never configuration-local resource visibility.
    /// The index is the same one that admitted every captured sidecar, including other profiles.
    pub(super) fn admits_new_asset_record(
        &self,
        record: &AssetRecord,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<bool, SourceInventoryError> {
        let mut maximum_path = record.path().as_str().len();
        for metadata in self.metadata.values() {
            let bytes = metadata.record().path().as_str().len();
            budget.charge_source_bytes(0, 0, bytes, 0)?;
            maximum_path = maximum_path.max(bytes);
        }
        let comparisons = (usize::BITS - self.metadata.len().leading_zeros()) as usize + 1;
        let scratch = product(maximum_path, 8)?;
        budget.charge_source_bytes(0, 0, product(scratch, comparisons)?, scratch)?;
        // The database owns Unicode case-folding, UUID and canonical path collision rules.
        // Drop any bounded conflict paths before releasing this probe's scratch reservation.
        let admitted = self.identity_catalogue.validate_insert(record).is_ok();
        release(budget, scratch)?;
        Ok(admitted)
    }

    pub(super) fn roots(&self) -> impl ExactSizeIterator<Item = &SourceRoot> {
        self.roots.values()
    }

    pub(super) fn missing_sidecars(&self) -> impl ExactSizeIterator<Item = &str> {
        self.missing_sidecars.keys().map(String::as_str)
    }

    pub(super) fn shared_custody(&self) -> SharedSourceInventoryCustody {
        SharedSourceInventoryCustody {
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

    pub(super) fn validate_authority(
        &self,
        schema: &AuthoringSchemaReceipt,
    ) -> Result<(), SourceInventoryError> {
        self.validate_manifest_observation(
            schema.root_identity(),
            schema.manifest_identity(),
            schema.manifest_digest(),
        )?;
        if self.bundle_identity != schema.bundle_identity()
            || self.bundle_digest != schema.bundle_digest()
        {
            return Err(SourceInventoryError::AuthorityChanged);
        }
        Ok(())
    }

    pub(super) fn validate_manifest_observation(
        &self,
        root: FileIdentity,
        manifest: FileIdentity,
        digest: ContentDigest,
    ) -> Result<(), SourceInventoryError> {
        if self.root_identity != root
            || self.manifest_identity != manifest
            || self.manifest_digest != digest
        {
            return Err(SourceInventoryError::AuthorityChanged);
        }
        Ok(())
    }

    pub(super) const fn bundle_digest(&self) -> ContentDigest {
        self.bundle_digest
    }

    pub(super) const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub(super) fn captured_open_sources(
        &self,
    ) -> impl ExactSizeIterator<Item = &AuthoringDocumentSourceFact> {
        self.open_sources.values()
    }

    pub(super) fn comparison_work(&self) -> Result<usize, SourceInventoryError> {
        let mut bytes = size_of::<Self>();
        for configuration in self.configurations() {
            bytes = sum(bytes, configuration.comparison_work()?)?;
        }
        for participant in self.participants() {
            bytes = sum(bytes, size_of::<SourceParticipant>())?;
            bytes = sum(bytes, participant.file().path().len())?;
            bytes = sum(bytes, participant.source_key().retained_bytes())?;
            bytes = sum(bytes, participant.bytes().len())?;
            bytes = sum(
                bytes,
                participant
                    .native_source()
                    .retained_bytes()
                    .ok_or(SourceInventoryError::AccountingOverflow)?,
            )?;
        }
        for metadata in self.metadata() {
            bytes = sum(bytes, size_of::<SourceMetadata>())?;
            bytes = sum(bytes, metadata.file().path().len())?;
            bytes = sum(bytes, metadata.bytes().len())?;
            bytes = sum(bytes, metadata.record().path().as_str().len())?;
            for policy in metadata.visibility() {
                bytes = sum(bytes, size_of::<ConfigurationRecordPolicy>())?;
                bytes = sum(
                    bytes,
                    policy
                        .scope()
                        .prefix()
                        .map_or(0, |path| path.as_str().len()),
                )?;
            }
            if let AssetSourceKind::Other(kind) = metadata.record().source_kind() {
                bytes = sum(bytes, kind.len())?;
            }
        }
        for (path, observation) in &self.file_observations {
            bytes = sum(bytes, sum(path.len(), size_of::<SourceFileObservation>())?)?;
            if let SourceFileObservation::Present { file, bytes: body } = observation {
                bytes = sum(bytes, sum(file.path().len(), body.len())?)?;
            }
        }
        for root in self.roots() {
            bytes = sum(bytes, size_of::<SourceRoot>())?;
            bytes = sum(bytes, root.path().len())?;
            if let SourceRootObservation::Missing { ancestor_path, .. } = root.observation() {
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
                    .ok_or(SourceInventoryError::AccountingOverflow)?,
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
        complete_open_sources: &[OpenSource<'_>],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceInventoryError> {
        self.validate_authority(schema)?;
        self.validate_observations(schema, complete_open_sources, budget)
    }
}

#[derive(Clone, Copy)]
struct Entry {
    kind: FileKind,
    identity: FileIdentity,
}

/// Finite borrowed physical observations. The operation admits their authority before scanning;
/// native policy construction always uses the actual parsed manifest and resolved configurations.
#[derive(Clone, Copy)]
pub(super) struct SourceInventoryInput<'a> {
    pub(super) root: &'a DirectoryCapability,
    pub(super) manifest_bytes: &'a [u8],
    pub(super) manifest_identity: FileIdentity,
    pub(super) manifest_digest: ContentDigest,
    pub(super) bundle_path: &'a str,
    pub(super) bundle_identity: FileIdentity,
    pub(super) bundle_digest: ContentDigest,
    pub(super) selected_settings: Option<&'a EffectiveProjectSettings>,
}

/// One required physical path/kind declaration with its original explicit native participation
/// marker. The marker preserves native context membership, grants no operation approval and
/// bypasses no source capability or identity checks. The scanner retains no recovery package.
/// The supplying owner proves sorted unique paths.
pub(super) struct SourceClaim<'a> {
    pub(super) path: &'a str,
    pub(super) kind: SourceDocumentKind,
    pub(super) explicit_native: bool,
}

/// A bounded exact leaf observation, independent of authored-source membership. Claims must be
/// sorted by unique path. Their actual files and existing parents join the inventory's custody.
pub(super) struct ObservedFileClaim<'a> {
    pub(super) path: &'a str,
    pub(super) maximum_bytes: usize,
}

/// Bounded grammar admission and its live parser reservation, without Ready/Pending approval.
/// The concrete caller validates authority before transferring this model to physical capture.
pub(super) struct SourceInventoryManifest {
    value: ProjectManifest,
    retained_bytes: usize,
}

impl SourceInventoryManifest {
    pub(super) fn parse(
        bytes: &[u8],
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceInventoryError> {
        let retained_bytes = product(bytes.len(), 128)?;
        reserve(budget, retained_bytes)?;
        budget.charge_source_bytes(0, bytes.len(), bytes.len(), 0)?;
        let load = ProjectManifest::parse_toml_bytes(bytes);
        if load.has_errors() {
            return Err(SourceInventoryError::ManifestInvalid);
        }
        Ok(Self {
            value: load.manifest.ok_or(SourceInventoryError::ManifestInvalid)?,
            retained_bytes,
        })
    }

    pub(super) fn value_mut(&mut self) -> &mut ProjectManifest {
        &mut self.value
    }
}

struct InventoryCapture<'a> {
    input: SourceInventoryInput<'a>,
    root: DirectoryCapability,
    explicit_claims: &'a [SourceClaim<'a>],
    observed_file_claims: &'a [ObservedFileClaim<'a>],
    budget: &'a mut AuthoringProjectAdmissionBudget,
    policies: Vec<ConfigurationPolicy>,
    roots: BTreeMap<String, SourceRoot>,
    claims: BTreeMap<String, SourceDocumentKind>,
    entries: BTreeMap<String, Entry>,
    directories: BTreeMap<String, FileIdentity>,
    metadata: BTreeMap<String, SourceMetadata>,
    file_observations: BTreeMap<String, SourceFileObservation>,
    database: ProjectAssetDatabase,
    spellings: BTreeMap<String, String>,
    identities: HashMap<FileIdentity, String>,
    open_sources: BTreeMap<String, AuthoringDocumentSourceFact>,
    missing_sidecars: BTreeMap<String, FileIdentity>,
    custody: InventoryCustody,
}

impl<'a> InventoryCapture<'a> {
    fn new(
        input: SourceInventoryInput<'a>,
        explicit_claims: &'a [SourceClaim<'a>],
        observed_file_claims: &'a [ObservedFileClaim<'a>],
        budget: &'a mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, SourceInventoryError> {
        for pair in observed_file_claims.windows(2) {
            budget.charge_source_bytes(0, 0, sum(pair[0].path.len(), pair[1].path.len())?, 0)?;
            if pair[0].path >= pair[1].path {
                return Err(SourceInventoryError::ObservedFileConflict);
            }
        }
        reserve(budget, size_of::<Self>())?;
        Ok(Self {
            input,
            root: input.root.share(),
            explicit_claims,
            observed_file_claims,
            budget,
            policies: Vec::new(),
            roots: BTreeMap::new(),
            claims: BTreeMap::new(),
            entries: BTreeMap::new(),
            directories: BTreeMap::new(),
            metadata: BTreeMap::new(),
            file_observations: BTreeMap::new(),
            database: ProjectAssetDatabase::default(),
            spellings: BTreeMap::new(),
            identities: HashMap::new(),
            open_sources: BTreeMap::new(),
            missing_sidecars: BTreeMap::new(),
            custody: InventoryCustody::new(input.root.share()),
        })
    }
}

impl InventoryCapture<'_> {
    fn is_observed_file(&mut self, path: &str) -> Result<bool, SourceInventoryError> {
        let claims = self.observed_file_claims;
        if claims.is_empty() {
            return Ok(false);
        }
        let depth = (usize::BITS - claims.len().leading_zeros()) as usize + 1;
        self.budget
            .charge_source_bytes(0, 0, product(path.len(), depth)?, 0)?;
        Ok(claims
            .binary_search_by(|claim| claim.path.cmp(path))
            .is_ok())
    }

    fn observe_files(
        &mut self,
        entries: &BTreeMap<String, Entry>,
    ) -> Result<(), SourceInventoryError> {
        let claims = self.observed_file_claims;
        for claim in claims {
            self.budget.charge_reference(claim.path.len(), 0, true)?;
            self.observe_spelling(claim.path)?;
            let lookup_depth =
                (usize::BITS - entries.len().max(self.roots.len()).leading_zeros()) as usize + 1;
            self.budget.charge_source_bytes(
                0,
                0,
                product(claim.path.len(), product(lookup_depth, 32)?)?,
                0,
            )?;
            let entry = entries.get(claim.path);
            if entry.is_some_and(|entry| entry.kind != FileKind::Regular)
                || self.roots.contains_key(claim.path)
            {
                return Err(SourceInventoryError::ObservedFileConflict);
            }
            // A missing intermediate directory is not a missing leaf. Pin the exact existing
            // parent before attempting the read; failed leaf reads refund only their own probe.
            let parent = self
                .custody
                .retain_directory(split_parent(claim.path).0, self.budget)?;
            reserve(
                self.budget,
                sum(
                    MAP_NODE_BYTES + size_of::<SourceFileObservation>(),
                    claim.path.len(),
                )?,
            )?;
            let observation = match self.read_source(
                claim.path,
                entry.map(|entry| entry.identity),
                claim.maximum_bytes,
            ) {
                Ok((file, bytes)) => {
                    self.observe_identity(file.identity, claim.path)?;
                    SourceFileObservation::Present { file, bytes }
                }
                Err(SourceInventoryError::Host(error)) if is_missing(&error) => {
                    if entry.is_some() {
                        return Err(SourceInventoryError::SourceChanged);
                    }
                    SourceFileObservation::Missing {
                        parent_identity: parent.identity(),
                    }
                }
                Err(error) => return Err(error),
            };
            self.file_observations
                .insert(claim.path.to_owned(), observation);
        }
        Ok(())
    }

    fn read_source(
        &mut self,
        path: &str,
        expected: Option<FileIdentity>,
        maximum: usize,
    ) -> Result<(SourceFileFact, Vec<u8>), SourceInventoryError> {
        let parent = self
            .custody
            .retain_directory(split_parent(path).0, self.budget)?;
        let (fact, bytes, capability, file_custody_bytes) =
            read_file(&self.root, path, expected, maximum, self.budget)?;
        if fact.parent_identity != Some(parent.identity()) {
            return Err(SourceInventoryError::SourceChanged);
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

    fn observe_root(&mut self, path: &str) -> Result<SourceRootObservation, SourceInventoryError> {
        match self.custody.retain_directory(path, self.budget) {
            Ok(directory) => {
                return Ok(SourceRootObservation::Available {
                    identity: directory.identity(),
                });
            }
            Err(SourceInventoryError::Host(error)) if is_missing(&error) => {}
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
        Ok(SourceRootObservation::Missing {
            ancestor_path: path[..ancestor_end].to_owned(),
            ancestor_identity,
        })
    }

    fn resolve_configurations(
        &mut self,
        parsed: SourceInventoryManifest,
    ) -> Result<(), SourceInventoryError> {
        let bytes = self.input.manifest_bytes;
        let SourceInventoryManifest {
            value: manifest,
            retained_bytes: scratch,
        } = parsed;
        // Reserve the complete table before allocating it. Per-profile string/binding copies
        // have their own admission before construction; resolution scratch remains temporary.
        let configurations = sum(manifest.profiles.len(), 1)?;
        reserve(
            self.budget,
            product(configurations, size_of::<ConfigurationPolicy>())?,
        )?;
        self.policies = Vec::with_capacity(configurations);
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
                .map_err(|_| SourceInventoryError::ProfileInvalid)?;
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
    ) -> Result<(), SourceInventoryError> {
        let paths = [
            &settings.paths.assets,
            &settings.paths.scenes,
            &settings.paths.prefabs,
            &settings.paths.scripts,
        ];
        self.policies
            .push(ConfigurationPolicy::new(settings, self.budget)?);
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
                    MAP_NODE_BYTES + size_of::<SourceRoot>(),
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
                SourceRoot {
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

    fn claim(&mut self, path: &str, kind: SourceDocumentKind) -> Result<(), SourceInventoryError> {
        if self.is_observed_file(path)? {
            return Err(SourceInventoryError::ObservedFileConflict);
        }
        let observations = self.observed_file_claims;
        if !observations.is_empty() {
            let depth = (usize::BITS - observations.len().leading_zeros()) as usize + 1;
            self.budget.charge_source_bytes(
                0,
                0,
                product(sum(path.len(), ".meta".len())?, depth)?,
                0,
            )?;
            // Compare the virtual sidecar name without allocating another path. Its exact
            // observation would otherwise hide metadata required by this original source.
            if observations
                .binary_search_by(|claim| {
                    claim.path.bytes().cmp(path.bytes().chain(".meta".bytes()))
                })
                .is_ok()
            {
                return Err(SourceInventoryError::ObservedFileConflict);
            }
        }
        self.budget.charge_reference(path.len(), 0, true)?;
        self.observe_spelling(path)?;
        if let Some(previous) = self.claims.get(path) {
            return if *previous == kind {
                Ok(())
            } else {
                Err(SourceInventoryError::KindConflict)
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
        sources: &[OpenSource<'_>],
    ) -> Result<(), SourceInventoryError> {
        for source in sources {
            let OpenSource::Bound(source) = source else {
                return Err(SourceInventoryError::UnboundOpenSource);
            };
            self.claim(source.relative_path(), source.source_key().kind())?;
            if let Some(previous) = self.open_sources.get(source.relative_path()) {
                if previous != *source {
                    return Err(SourceInventoryError::OpenSourceChanged);
                }
                continue;
            }
            let bytes = source
                .checked_retained_bytes()
                .ok_or(SourceInventoryError::AccountingOverflow)?;
            reserve(
                self.budget,
                sum(MAP_NODE_BYTES + source.relative_path().len(), bytes)?,
            )?;
            self.open_sources
                .insert(source.relative_path().to_owned(), (*source).clone());
        }
        Ok(())
    }

    fn enumerate(&mut self) -> Result<(), SourceInventoryError> {
        let mut pending = BTreeMap::new();
        for (path, root) in &self.roots {
            if let SourceRootObservation::Available { identity } = root.observation {
                reserve(self.budget, sum(MAP_NODE_BYTES, path.len())?)?;
                pending.insert(path.clone(), identity);
            }
        }
        while let Some((path, identity)) = pending.pop_first() {
            if let Some(previous) = self.directories.get(&path) {
                if previous != &identity {
                    return Err(SourceInventoryError::SourceChanged);
                }
                continue;
            }
            self.observe_identity(identity, &path)?;
            let depth = RelativePath::preflight(&path)?.components();
            let directory = self.custody.retain_directory(&path, self.budget)?;
            if directory.identity() != identity {
                return Err(SourceInventoryError::SourceChanged);
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
                        return Err(SourceInventoryError::EnumerationIncomplete);
                    }
                    for entry in batch.entries() {
                        let name = entry
                            .name()
                            .to_str()
                            .ok_or(SourceInventoryError::EnumerationIncomplete)?;
                        let length = sum(sum(path.len(), 1)?, name.len())?;
                        self.budget.charge_reference(length, depth, false)?;
                        reserve(
                            self.budget,
                            sum(product(length, 3)?, MAP_NODE_BYTES + size_of::<Entry>())?,
                        )?;
                        let child = format!("{path}/{name}");
                        let identity = entry
                            .identity()
                            .ok_or(SourceInventoryError::EnumerationIncomplete)?;
                        if entry.kind() == FileKind::Directory {
                            let directory = self.custody.retain_directory(&child, self.budget)?;
                            if directory.identity() != identity {
                                return Err(SourceInventoryError::SourceChanged);
                            }
                        }
                        self.observe_spelling(&child)?;
                        self.observe_identity(identity, &child)?;
                        if let Some(previous) = self.entries.get(&child) {
                            if previous.identity != identity || previous.kind != entry.kind() {
                                return Err(SourceInventoryError::SourceChanged);
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
                            _ => return Err(SourceInventoryError::EnumerationIncomplete),
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
    ) -> Result<(), SourceInventoryError> {
        // A present sidecar's first open is its actual retained read; never probe and close it
        // before reopening the same identity for classification.
        match self.index_metadata(metadata_path, None) {
            Ok(()) => Ok(()),
            Err(SourceInventoryError::Host(error)) if is_missing(&error) => {
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
    ) -> Result<(), SourceInventoryError> {
        if self.is_observed_file(path)? {
            return Err(SourceInventoryError::ObservedFileConflict);
        }
        let limits = AssetMetaFileLimits::default();
        let (file, bytes) = self.read_source(path, expected, limits.encoded_bytes().get())?;
        let source_path = path
            .strip_suffix(".meta")
            .ok_or(SourceInventoryError::KindConflict)?;
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
                return Err(SourceInventoryError::MetadataFormat {
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
            .map_err(|_| SourceInventoryError::MetadataPathMismatch)?;
        // One possible declaration scope per actual configuration. Allocate the complete table
        // only after its original operation allowance accepts it; each scope copy is admitted
        // separately before cloning its prefix. Visibility cannot stop at the first match.
        reserve(
            self.budget,
            product(self.policies.len(), size_of::<ConfigurationRecordPolicy>())?,
        )?;
        let mut visibility = Vec::with_capacity(self.policies.len());
        let external = self.open_sources.contains_key(source_path)
            || self.explicit_native_claim(source_path)?;
        let mut catalogue_source = false;
        let mut catalogue_binding = None;
        let mut native_binding = None;
        for (index, policy) in self.policies.iter().enumerate() {
            let mut in_catalogue = false;
            let mut matched = false;
            for (binding_index, binding) in policy.catalogue_bindings.iter().enumerate() {
                self.budget.charge_source_bytes(
                    0,
                    0,
                    sum(source_path.len(), binding.directory().as_str().len())?,
                    0,
                )?;
                let local = binding.scope().relative_path(&canonical_path);
                in_catalogue |= local.is_some();
                if local == Some(candidate.identity().path().as_str()) {
                    catalogue_binding.get_or_insert((index, binding_index));
                    reserve(
                        self.budget,
                        binding
                            .scope()
                            .prefix()
                            .map_or(0, |path| path.as_str().len()),
                    )?;
                    visibility.push(ConfigurationRecordPolicy {
                        configuration: index,
                        scope: binding.scope().clone(),
                        catalogue_binding: Some(binding_index),
                    });
                    matched = true;
                    break;
                }
            }
            catalogue_source |= in_catalogue;
            // Invalid catalogue declarations cannot fall back to Scripts in that configuration.
            // Other actual configurations may still admit the same physical sidecar, including
            // a native-only context when an open source or explicit native claim lives outside
            // catalogue roots.
            if !matched && !in_catalogue && external {
                for (scope_index, scope) in policy.native_scopes.iter().enumerate() {
                    self.budget.charge_source_bytes(
                        0,
                        0,
                        sum(source_path.len(), scope.len())?,
                        0,
                    )?;
                    if below(source_path, scope) == Some(candidate.identity().path().as_str()) {
                        native_binding.get_or_insert((index, scope_index));
                        reserve(self.budget, scope.len())?;
                        let prefix = AssetSourcePath::new(scope.as_str())
                            .map_err(|_| SourceInventoryError::MetadataPathMismatch)?;
                        visibility.push(ConfigurationRecordPolicy {
                            configuration: index,
                            scope: AssetPathScope::under(prefix),
                            catalogue_binding: None,
                        });
                        break;
                    }
                }
            }
        }
        // Preserve the catalogue owner's existing rejection: a row discovered under any
        // catalogue root needs an actual catalogue admission, never a native-only escape.
        if catalogue_binding.is_none() && (catalogue_source || native_binding.is_none()) {
            return Err(SourceInventoryError::MetadataPathMismatch);
        }
        let identity_bytes = candidate
            .retained_bytes()
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        let record_bytes = sum(
            product(identity_bytes, 2)?,
            sum(
                product(source_path.len(), 12)?,
                MAP_NODE_BYTES * 4 + size_of::<SourceMetadata>() + size_of::<AssetRecord>() * 3,
            )?,
        )?;
        reserve(self.budget, record_bytes)?;
        // The original bounded read is already retained. Move that same encoded allocation
        // into the inventory so consumers never reopen or reconstruct a sidecar's settings.
        let authored_path = candidate.identity().path().clone();
        let (identity, bytes) = candidate.into_parts();
        let (policy, record) = if let Some((policy, binding)) = catalogue_binding {
            let scope = self.policies[policy].catalogue_bindings[binding].scope();
            (policy, identity.into_record(scope))
        } else {
            let (policy, scope_index) =
                native_binding.ok_or(SourceInventoryError::MetadataPathMismatch)?;
            let prefix =
                AssetSourcePath::new(self.policies[policy].native_scopes[scope_index].as_str())
                    .map_err(|_| SourceInventoryError::MetadataPathMismatch)?;
            let scope = AssetPathScope::under(prefix);
            (policy, identity.into_record(&scope))
        };
        if record.path() != &canonical_path {
            return Err(SourceInventoryError::MetadataPathMismatch);
        }
        // All metadata participates in global identity admission, including non-document records.
        // Reuse the catalogue's database owner for stable ID, canonical path and kind conflicts.
        self.database.insert(record.clone())?;
        self.metadata.insert(
            source_path.to_owned(),
            SourceMetadata {
                file,
                record,
                authored_path,
                bytes,
                policy,
                visibility,
            },
        );
        release(self.budget, scratch)?;
        Ok(())
    }

    fn explicit_native_claim(&mut self, path: &str) -> Result<bool, SourceInventoryError> {
        let members = self.explicit_claims;
        if members.is_empty() {
            return Ok(false);
        }
        // The supplying owner already proves this list sorted and unique. Charge the bounded
        // borrowed search rather than cloning another index or rescanning every member per file.
        let depth = (usize::BITS - members.len().leading_zeros()) as usize + 1;
        self.budget
            .charge_source_bytes(0, 0, product(path.len(), depth)?, 0)?;
        Ok(members
            .binary_search_by(|member| member.path.cmp(path))
            .is_ok_and(|index| members[index].explicit_native))
    }

    fn file_kind(
        &mut self,
        path: &str,
    ) -> Result<Option<SourceDocumentKind>, SourceInventoryError> {
        validation::source_file_kind(
            path,
            &self.roots,
            self.metadata.get(path).map(|metadata| &metadata.record),
            self.budget,
        )
    }

    fn revalidate(
        &mut self,
        participants: &BTreeMap<String, SourceParticipant>,
    ) -> Result<(), SourceInventoryError> {
        validation::revalidate_source_observations(
            &self.root,
            &self.roots,
            &self.custody,
            participants
                .values()
                .map(|source| &source.file)
                .chain(self.metadata.values().map(|metadata| &metadata.file)),
            &self.missing_sidecars,
            &self.file_observations,
            self.budget,
        )
    }

    fn observe_spelling(&mut self, path: &str) -> Result<(), SourceInventoryError> {
        // Reuse the source index's conservative Unicode lowercase rule. Reserve the maximum
        // expansion before allocating a key, then use an ordered index rather than pair scans.
        let bytes = product(path.len(), 3)?;
        reserve(self.budget, bytes)?;
        self.budget.charge_source_bytes(0, 0, bytes, 0)?;
        let key = path.to_lowercase();
        if let Some(previous) = self.spellings.get(&key) {
            if previous != path {
                return Err(SourceInventoryError::PathConflict);
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
    ) -> Result<(), SourceInventoryError> {
        if let Some(previous) = self.identities.get(&identity) {
            return if previous == path {
                Ok(())
            } else {
                Err(SourceInventoryError::IdentityConflict)
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

pub(super) fn read_file(
    root: &DirectoryCapability,
    path: &str,
    expected: Option<FileIdentity>,
    maximum: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(SourceFileFact, Vec<u8>, Arc<FileCapability>, usize), SourceInventoryError> {
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
) -> Result<(SourceFileFact, Vec<u8>, Arc<FileCapability>, usize), SourceInventoryError> {
    let relative = relative_path(path, budget)?;
    let custody_bytes = size_of::<FileCapability>() + 2 * size_of::<usize>();
    reserve(budget, custody_bytes)?;
    budget.retain_open_handles(1)?;
    budget.charge_open_file(HANDLE_PEAK)?;
    let file = open_file_exact(root, &relative, budget)?;
    if expected.is_some_and(|expected| file.identity() != expected) {
        return Err(SourceInventoryError::SourceChanged);
    }
    let (limit, _) = budget.read_limit(maximum)?;
    // Stream once using bounded stack storage, then reserve the exact body and copying work
    // before the filesystem owner allocates. A same-length concurrent rewrite must also hash equal.
    let digest = bounded_digest(&file, limit, budget)?;
    let length =
        usize::try_from(digest.length()).map_err(|_| SourceInventoryError::AccountingOverflow)?;
    reserve(
        budget,
        sum(
            product(length, 2)?,
            sum(size_of::<SourceFileFact>(), path.len())?,
        )?,
    )?;
    budget.charge_source_bytes(length, 0, length, 0)?;
    let bytes = file.read_to_end_bounded(digest.length())?;
    if ContentDigest::of_bytes(&bytes) != digest {
        return Err(SourceInventoryError::SourceChanged);
    }
    Ok((
        SourceFileFact {
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

pub(super) fn revalidate_file(
    root: &DirectoryCapability,
    fact: &SourceFileFact,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceInventoryError> {
    revalidate_file_facts(
        root,
        &fact.path,
        fact.identity,
        fact.parent_identity,
        fact.digest,
        budget,
    )
}

pub(super) fn revalidate_file_facts(
    root: &DirectoryCapability,
    path: &str,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    digest: ContentDigest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceInventoryError> {
    let relative = relative_path(path, budget)?;
    budget.admit_handle_peak(HANDLE_PEAK)?;
    let file = open_file_exact(root, &relative, budget)?;
    if file.identity() != identity
        || file.parent_identity() != parent_identity
        || bounded_digest(
            &file,
            usize::try_from(digest.length())
                .map_err(|_| SourceInventoryError::AccountingOverflow)?,
            budget,
        )? != digest
    {
        return Err(SourceInventoryError::SourceChanged);
    }
    Ok(())
}

fn bounded_digest(
    file: &FileCapability,
    maximum: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ContentDigest, SourceInventoryError> {
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
    // Keep one unit for the finite over-limit/EOF probe. Errors still spend the admitted
    // read bound: the filesystem error does not report how much of that bound it consumed.
    let limit = maximum.min(remaining - 1);
    match file.digest(limit as u64) {
        Ok(digest) => {
            let bytes = usize::try_from(digest.length())
                .map_err(|_| SourceInventoryError::AccountingOverflow)?;
            budget.charge_source_bytes(0, 0, sum(bytes, 1)?, 0)?;
            Ok(digest)
        }
        Err(error) => {
            budget.charge_source_bytes(0, 0, sum(limit, 1)?, 0)?;
            if matches!(error, FsError::ByteLimitExceeded { .. }) && limit < maximum {
                Err(AuthoringProjectAdmissionError::DecodeWorkExceeded {
                    maximum: budget.limits.decode_work().get(),
                }
                .into())
            } else {
                Err(error.into())
            }
        }
    }
}

fn observe_root(
    root: &DirectoryCapability,
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceRootObservation, SourceInventoryError> {
    let relative = relative_path(path, budget)?;
    budget.charge_open_file(HANDLE_PEAK)?;
    match open_directory_exact(root, &relative, budget) {
        Ok(directory) => {
            return Ok(SourceRootObservation::Available {
                identity: directory.identity(),
            });
        }
        Err(SourceInventoryError::Host(error)) if is_missing(&error) => {}
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
            Err(SourceInventoryError::Host(error)) if is_missing(&error) => {
                reserve(budget, ancestor_end)?;
                return Ok(SourceRootObservation::Missing {
                    ancestor_path: path[..ancestor_end].to_owned(),
                    ancestor_identity,
                });
            }
            Err(error) => return Err(error),
        }
    }
    Err(SourceInventoryError::SourceChanged)
}

fn relative_path(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<RelativePath, SourceInventoryError> {
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
    open: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, SourceInventoryError>,
) -> Result<T, SourceInventoryError> {
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
) -> Result<FileCapability, SourceInventoryError> {
    budget.admit_handle_peak(HANDLE_PEAK)?;
    with_native_open_scratch(budget, |_| root.open_file_exact(path).map_err(Into::into))
}

fn open_directory_exact(
    root: &DirectoryCapability,
    path: &RelativePath,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<DirectoryCapability, SourceInventoryError> {
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
    ) -> Result<Self, SourceInventoryError> {
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
    ) -> Result<(), SourceInventoryError> {
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

pub(super) fn is_missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

pub(super) fn reserve(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: usize,
) -> Result<(), SourceInventoryError> {
    budget.charge_source_bytes(0, 0, 0, bytes)?;
    Ok(())
}

pub(super) fn with_candidate_storage<T, E: From<SourceInventoryError>>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, E>,
) -> Result<T, E> {
    let retained = budget.retained_candidate_bytes;
    let handles = budget.retained_open_handles;
    let result = prepare(budget);
    if result.is_err() {
        // All candidate locals have dropped before this point. Never undo cumulative work or
        // file/shape admission when refunding storage from a failed preparation.
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(retained)
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        release(budget, temporary)?;
        let closed = budget
            .retained_open_handles
            .checked_sub(handles)
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        budget.release_open_handles(closed);
    }
    result
}

pub(super) fn with_temporary_storage<E: From<SourceInventoryError>>(
    budget: &mut AuthoringProjectAdmissionBudget,
    inspect: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<(), E>,
) -> Result<(), E> {
    let retained = budget.retained_candidate_bytes;
    let handles = budget.retained_open_handles;
    let result = inspect(budget);
    let temporary = budget
        .retained_candidate_bytes
        .checked_sub(retained)
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    release(budget, temporary)?;
    let closed = budget
        .retained_open_handles
        .checked_sub(handles)
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    budget.release_open_handles(closed);
    result
}

pub(super) fn release(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: usize,
) -> Result<(), SourceInventoryError> {
    budget.reconcile_retained_reservation(bytes, 0)?;
    Ok(())
}

pub(super) fn sum(left: usize, right: usize) -> Result<usize, SourceInventoryError> {
    left.checked_add(right)
        .ok_or(SourceInventoryError::AccountingOverflow)
}

pub(super) fn product(left: usize, right: usize) -> Result<usize, SourceInventoryError> {
    left.checked_mul(right)
        .ok_or(SourceInventoryError::AccountingOverflow)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceInventoryError {
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
    #[error("exact file observations overlap or conflict with required inventory declarations")]
    ObservedFileConflict,
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
    #[error("source inventory accounting overflowed")]
    AccountingOverflow,
}

impl From<AuthoringProjectAdmissionError> for SourceInventoryError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Self::Admission(error)
    }
}

impl From<AuthoringProjectSourceAdmissionError> for SourceInventoryError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self::Native(error)
    }
}

impl From<FsError> for SourceInventoryError {
    fn from(error: FsError) -> Self {
        Self::Host(error)
    }
}

impl From<PathValidationError> for SourceInventoryError {
    fn from(error: PathValidationError) -> Self {
        Self::Path(error)
    }
}

impl From<AssetDatabaseError> for SourceInventoryError {
    fn from(error: AssetDatabaseError) -> Self {
        Self::Catalogue(error)
    }
}
