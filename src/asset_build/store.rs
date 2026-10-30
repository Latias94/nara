//! A cooperating, capability-relative local content store. The index is the visibility boundary.

use std::{
    io::{Seek, SeekFrom, Write},
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use nara_fs::{
    DirectoryCapability, DirectorySyncReceipt, DirectorySyncTier, ExpectedTarget, FileCapability,
    FsError, FsOperation, LockGuarantee, LockMode, LockedFileCapability, RelativeComponent,
    ReplaceReceipt, TemporaryFileFailure,
};

use super::{
    budget::{ArtifactBuildBudget, ArtifactBuildLease, add, mul},
    format::{self, GroupManifest, RecipeEntry, StoreIndex},
    model::{
        ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildSeed,
        ArtifactContentDigest, ArtifactGroupCandidate, ArtifactGroupMember, ArtifactRecipeDigest,
    },
};

const LOCK_NAME: &str = "artifact-store.lock";
const INDEX_NAME: &str = "artifact-index.json";
// nara_fs digest verification uses a bounded 64 KiB buffer. One operation reuses this logical
// scratch allowance across sequential reads, hashing and capability-name construction.
const OPERATION_SCRATCH: usize = 64 * 1024 + 4096;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Finite request fences. `Reuse` is also available to the executing owner before returning a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactPublicationPoint {
    BeforeBlob { index: usize },
    AfterBlob { index: usize },
    BeforeManifest,
    BeforeIndex,
    AfterIndex,
    Reuse,
}

/// Exact verified bytes and metadata with one shared retained charge across all clones.
#[derive(Debug, Clone)]
pub struct VerifiedArtifactGroup(Arc<VerifiedGroupInner>);

#[derive(Debug)]
struct VerifiedGroupInner {
    digest: ArtifactContentDigest,
    manifest: GroupManifest,
    bytes: Vec<Arc<[u8]>>,
    retained_bytes: usize,
    _lease: ArtifactBuildLease,
}

impl VerifiedArtifactGroup {
    #[must_use]
    pub fn digest(&self) -> ArtifactContentDigest {
        self.0.digest
    }
    #[must_use]
    pub fn recipe_digest(&self) -> ArtifactRecipeDigest {
        self.0.manifest.recipe
    }
    #[must_use]
    pub fn seed(&self) -> &ArtifactBuildSeed {
        &self.0.manifest.seed
    }
    #[must_use]
    pub fn observations(&self) -> &[ArtifactBuildObservation] {
        &self.0.manifest.observations
    }
    #[must_use]
    pub fn members(&self) -> &[ArtifactGroupMember] {
        &self.0.manifest.members
    }
    #[must_use]
    pub fn member_bytes(&self, name: &str) -> Option<&[u8]> {
        let index = self
            .members()
            .binary_search_by(|member| member.name.as_str().cmp(name))
            .ok()?;
        self.0.bytes.get(index).map(AsRef::as_ref)
    }
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.0.retained_bytes
    }
}

/// Actual index publication evidence. Unsupported directory sync is explicit and has no receipt.
///
/// The native lock serializes cooperating owners; even an enforced byte-range lock does not make
/// generated names immutable or protect the directory from a writer ignoring this protocol.
#[derive(Debug)]
pub struct ArtifactCommitReceipt {
    group: VerifiedArtifactGroup,
    index: ReplaceReceipt,
    directory_sync: Option<DirectorySyncReceipt>,
    directory_sync_tier: DirectorySyncTier,
    lock_guarantee: LockGuarantee,
}

impl ArtifactCommitReceipt {
    #[must_use]
    pub fn group(&self) -> &VerifiedArtifactGroup {
        &self.group
    }
    #[must_use]
    pub fn into_group(self) -> VerifiedArtifactGroup {
        self.group
    }
    #[must_use]
    pub const fn index_receipt(&self) -> &ReplaceReceipt {
        &self.index
    }
    #[must_use]
    pub const fn directory_sync(&self) -> Option<DirectorySyncReceipt> {
        self.directory_sync
    }
    #[must_use]
    pub const fn directory_sync_tier(&self) -> DirectorySyncTier {
        self.directory_sync_tier
    }
    #[must_use]
    pub const fn lock_guarantee(&self) -> LockGuarantee {
        self.lock_guarantee
    }
}

/// A generated-store owner retaining its original root session and live lock-file binding.
pub struct ArtifactBuildStore {
    root: DirectoryCapability,
    original_lock: FileCapability,
    budget: Arc<ArtifactBuildBudget>,
    limits: ArtifactBuildLimits,
}

impl ArtifactBuildStore {
    pub fn open(
        root: DirectoryCapability,
        limits: ArtifactBuildLimits,
    ) -> Result<Self, ArtifactBuildError> {
        let budget = ArtifactBuildBudget::new(limits);
        let name = component(LOCK_NAME)?;
        let original_lock = match root.open_child_file(&name) {
            Ok(file) => file,
            Err(error) if missing(&error) => match root.create_child_file(&name) {
                Ok(file) => {
                    file.sync()?;
                    file
                }
                Err(FsError::AlreadyExists { .. }) => root.open_child_file(&name)?,
                Err(error) => return Err(error.into()),
            },
            Err(error) => return Err(error.into()),
        };
        let store = Self {
            root,
            original_lock,
            budget,
            limits,
        };
        let locked = store.lock()?;
        let _snapshot = store.read_index()?;
        store.validate_lock(&locked)?;
        Ok(store)
    }

    #[must_use]
    pub const fn limits(&self) -> ArtifactBuildLimits {
        self.limits
    }

    pub(super) fn reserve_work(
        &self,
        bytes: usize,
    ) -> Result<ArtifactBuildLease, ArtifactBuildError> {
        self.budget.reserve_work(bytes)
    }

    pub(super) fn reserve_retained(
        &self,
        bytes: usize,
    ) -> Result<ArtifactBuildLease, ArtifactBuildError> {
        self.budget.reserve_retained(bytes)
    }

    /// Returns a completely verified stored hint. Only the executing owner can replay current
    /// observations and promote this hint into a current-input hit.
    pub fn lookup_seed(
        &self,
        seed: &ArtifactBuildSeed,
    ) -> Result<Option<VerifiedArtifactGroup>, ArtifactBuildError> {
        format::validate_seed(seed, self.limits)?;
        let _scratch = self.reserve_work(OPERATION_SCRATCH)?;
        let digest = seed.digest()?;
        let locked = self.lock()?;
        let snapshot = self.read_index()?;
        let Some(hint) = snapshot.index.hints.iter().find(|hint| hint.seed == digest) else {
            return Ok(None);
        };
        let entry = snapshot
            .index
            .recipes
            .iter()
            .find(|entry| entry.recipe == hint.recipe)
            .ok_or(ArtifactBuildError::InvalidCache(
                "seed hint has no successful recipe",
            ))?;
        let group = self.load_group(entry)?;
        if group.seed() != seed {
            return Err(ArtifactBuildError::InvalidCache(
                "seed hint does not match the selected group",
            ));
        }
        self.validate_lock(&locked)?;
        self.validate_index_binding(&snapshot)?;
        Ok(Some(group))
    }

    pub fn commit(
        &self,
        mut candidate: ArtifactGroupCandidate,
        mut control: impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
    ) -> Result<ArtifactCommitReceipt, ArtifactBuildError> {
        // All ownership requests below precede the corresponding vectors or encoded buffers.
        let _candidate_work =
            self.reserve_work(format::candidate_work(&candidate, self.limits)?)?;
        let _scratch = self.reserve_work(OPERATION_SCRATCH)?;
        format::canonicalize_candidate(&mut candidate, self.limits)?;
        let mut members = Vec::with_capacity(candidate.outputs.len());
        let mut bytes = Vec::with_capacity(candidate.outputs.len());
        for output in candidate.outputs {
            members.push(ArtifactGroupMember {
                name: output.name,
                digest: ArtifactContentDigest::of_bytes(&output.bytes),
                format: output.format,
                format_version: output.format_version,
                products: output.products,
                dependencies: output.dependencies,
            });
            bytes.push(output.bytes);
        }
        let recipe = candidate.seed.recipe_digest(&candidate.observations)?;
        let manifest = GroupManifest {
            seed: candidate.seed,
            observations: candidate.observations,
            recipe,
            members,
        };
        let encoded = format::encode_group(&manifest, self)?;
        let digest = ArtifactContentDigest::of_bytes(&encoded.bytes);
        let locked = self.lock()?;
        let mut snapshot = self.read_index()?;
        let existing = snapshot
            .index
            .recipes
            .iter()
            .find(|entry| entry.recipe == recipe);
        let group = if let Some(entry) = existing {
            // Never trust an association (or compare its digest for determinism) until its whole
            // stored group has been independently read and verified.
            let existing = self.load_group(entry)?;
            if existing.digest() != digest {
                return Err(ArtifactBuildError::DeterminismConflict);
            }
            existing
        } else {
            let retained_bytes = retained_size(&manifest)?;
            let lease = self.reserve_retained(retained_bytes)?;
            VerifiedArtifactGroup(Arc::new(VerifiedGroupInner {
                digest,
                manifest,
                bytes,
                retained_bytes,
                _lease: lease,
            }))
        };
        let seed_digest = group.seed().digest()?;
        let _index_growth = self.reserve_work(format::index_growth_work(&snapshot.index)?)?;
        snapshot
            .index
            .associate(seed_digest, recipe, digest, self.limits)?;
        let index_bytes = format::encode_index(&snapshot.index, self)?;

        for (index, member) in group.members().iter().enumerate() {
            invoke_control(&mut control, ArtifactPublicationPoint::BeforeBlob { index })?;
            self.validate_lock(&locked)?;
            self.publish_object(
                &blob_name(member.digest)?,
                group.0.bytes[index].as_ref(),
                member.digest,
            )?;
            invoke_control(&mut control, ArtifactPublicationPoint::AfterBlob { index })?;
        }
        invoke_control(&mut control, ArtifactPublicationPoint::BeforeManifest)?;
        self.validate_lock(&locked)?;
        self.publish_object(&group_name(digest)?, &encoded.bytes, digest)?;
        invoke_control(&mut control, ArtifactPublicationPoint::BeforeIndex)?;
        self.validate_lock(&locked)?;
        self.validate_index_binding(&snapshot)?;
        self.verify_published_group(&group)?;
        let expected = snapshot
            .file
            .as_ref()
            .map_or(ExpectedTarget::Missing, |file| {
                ExpectedTarget::Identity(file.identity())
            });
        let publication = self.publish(&component(INDEX_NAME)?, &index_bytes.bytes, expected)?;
        let mut receipt = ArtifactCommitReceipt {
            group,
            index: publication,
            directory_sync: None,
            directory_sync_tier: match nara_fs::platform_capability_matrix().directory_sync_tier() {
                DirectorySyncTier::Unsupported => DirectorySyncTier::Unsupported,
                _ => DirectorySyncTier::Unproven,
            },
            lock_guarantee: locked.guarantee(),
        };
        // No fallible path beyond the visibility boundary may return an ordinary rejection.
        let completion = catch_unwind(AssertUnwindSafe(|| {
            invoke_control(&mut control, ArtifactPublicationPoint::AfterIndex)?;
            if receipt.index.verified_content_digest()
                != Some(ArtifactContentDigest::of_bytes(&index_bytes.bytes).content())
            {
                return Err(ArtifactBuildError::InvalidCache(
                    "published index verification failed",
                ));
            }
            match self.root.sync() {
                Ok(sync) => {
                    receipt.directory_sync_tier = sync.tier();
                    receipt.directory_sync = Some(sync);
                }
                Err(FsError::Unsupported {
                    operation: FsOperation::SyncDirectory,
                    ..
                }) => {
                    receipt.directory_sync_tier = DirectorySyncTier::Unsupported;
                }
                Err(error) => return Err(error.into()),
            }
            self.validate_lock(&locked)?;
            self.verify_published_group(receipt.group())?;
            let index = self.root.open_child_file(&component(INDEX_NAME)?)?;
            if index.identity() != receipt.index.candidate_identity() {
                return Err(ArtifactBuildError::InvalidCache(
                    "published index binding changed",
                ));
            }
            index.verify_digest(
                ArtifactContentDigest::of_bytes(&index_bytes.bytes).content(),
                index_bytes.bytes.len() as u64,
            )?;
            Ok(())
        }))
        .unwrap_or(Err(ArtifactBuildError::Panicked));
        match completion {
            Ok(()) => Ok(receipt),
            Err(cause) => Err(ArtifactBuildError::Uncertain {
                receipt: Box::new(receipt),
                cause: Box::new(cause),
            }),
        }
    }

    fn lock(&self) -> Result<LockedFileCapability, ArtifactBuildError> {
        let file = self.root.open_child_file(&component(LOCK_NAME)?)?;
        if file.identity() != self.original_lock.identity()
            || file.parent_identity() != Some(self.root.identity())
            || !file.same_object(&self.original_lock)?
        {
            return Err(ArtifactBuildError::Stale);
        }
        let locked = file
            .try_lock(LockMode::Exclusive)
            .map_err(ArtifactBuildError::Lock)?;
        self.validate_lock(&locked)?;
        Ok(locked)
    }

    fn validate_lock(&self, locked: &LockedFileCapability) -> Result<(), ArtifactBuildError> {
        let live = self.root.open_child_file(&component(LOCK_NAME)?)?;
        if live.identity() != self.original_lock.identity()
            || locked.identity() != self.original_lock.identity()
            || locked.parent_identity() != Some(self.root.identity())
            || live.parent_identity() != Some(self.root.identity())
            || !live.same_object(&self.original_lock)?
        {
            return Err(ArtifactBuildError::Stale);
        }
        Ok(())
    }

    fn read_index(&self) -> Result<IndexSnapshot, ArtifactBuildError> {
        let file = match self.root.open_child_file(&component(INDEX_NAME)?) {
            Ok(file) => file,
            Err(error) if missing(&error) => {
                return Ok(IndexSnapshot {
                    index: StoreIndex::default(),
                    file: None,
                    digest: None,
                    _work: None,
                });
            }
            Err(error) => return Err(error.into()),
        };
        let encoded = self.read_file(&file, self.limits.metadata_bytes.get(), true)?;
        let digest = ArtifactContentDigest::of_bytes(&encoded.bytes);
        let decoded = format::decode_index(&encoded.bytes, self)?;
        Ok(IndexSnapshot {
            index: decoded.value,
            file: Some(file),
            digest: Some(digest),
            _work: Some(decoded.lease),
        })
    }

    fn validate_index_binding(&self, snapshot: &IndexSnapshot) -> Result<(), ArtifactBuildError> {
        match (
            self.root.open_child_file(&component(INDEX_NAME)?),
            snapshot.file.as_ref(),
            snapshot.digest,
        ) {
            (Err(error), None, None) if missing(&error) => Ok(()),
            (Ok(current), Some(original), Some(digest))
                if current.identity() == original.identity() =>
            {
                current.verify_digest(digest.content(), digest.length())?;
                Ok(())
            }
            (Err(error), _, _) => Err(error.into()),
            _ => Err(ArtifactBuildError::Stale),
        }
    }

    fn load_group(&self, entry: &RecipeEntry) -> Result<VerifiedArtifactGroup, ArtifactBuildError> {
        let file = self
            .root
            .open_child_file(&group_name(entry.group)?)
            .map_err(|_| {
                ArtifactBuildError::InvalidCache("referenced group is missing or inaccessible")
            })?;
        let encoded = self.read_file(&file, self.limits.metadata_bytes.get(), true)?;
        if ArtifactContentDigest::of_bytes(&encoded.bytes) != entry.group {
            return Err(ArtifactBuildError::InvalidCache(
                "group content digest mismatch",
            ));
        }
        let decoded = format::decode_group(&encoded.bytes, self)?;
        if decoded.value.recipe != entry.recipe {
            return Err(ArtifactBuildError::InvalidCache(
                "recipe association does not match group",
            ));
        }
        let retained_bytes = retained_size(&decoded.value)?;
        let retained = self.reserve_retained(retained_bytes)?;
        let mut bytes = Vec::with_capacity(decoded.value.members.len());
        for member in &decoded.value.members {
            let file = self
                .root
                .open_child_file(&blob_name(member.digest)?)
                .map_err(|_| {
                    ArtifactBuildError::InvalidCache("referenced blob is missing or inaccessible")
                })?;
            let expected_len = usize::try_from(member.digest.length())
                .map_err(|_| ArtifactBuildError::Overflow)?;
            // Account for the temporary Vec and Arc conversion before either allocation.
            let _copy = self.reserve_work(expected_len)?;
            let captured = self.read_file(&file, expected_len, true)?;
            if ArtifactContentDigest::of_bytes(&captured.bytes) != member.digest {
                return Err(ArtifactBuildError::InvalidCache(
                    "blob content digest mismatch",
                ));
            }
            bytes.push(Arc::from(captured.bytes));
        }
        Ok(VerifiedArtifactGroup(Arc::new(VerifiedGroupInner {
            digest: entry.group,
            manifest: decoded.value,
            bytes,
            retained_bytes,
            _lease: retained,
        })))
    }

    fn verify_published_group(
        &self,
        group: &VerifiedArtifactGroup,
    ) -> Result<(), ArtifactBuildError> {
        let manifest = self
            .root
            .open_child_file(&group_name(group.digest())?)
            .map_err(|_| ArtifactBuildError::InvalidCache("published group disappeared"))?;
        manifest
            .verify_digest(group.digest().content(), group.digest().length())
            .map_err(|_| ArtifactBuildError::InvalidCache("published group changed"))?;
        for member in group.members() {
            let file = self
                .root
                .open_child_file(&blob_name(member.digest)?)
                .map_err(|_| ArtifactBuildError::InvalidCache("published blob disappeared"))?;
            file.verify_digest(member.digest.content(), member.digest.length())
                .map_err(|_| ArtifactBuildError::InvalidCache("published blob changed"))?;
        }
        Ok(())
    }

    fn read_file(
        &self,
        file: &FileCapability,
        maximum: usize,
        cache: bool,
    ) -> Result<format::Encoded, ArtifactBuildError> {
        let len = file.reader()?.seek(SeekFrom::End(0)).map_err(|source| {
            ArtifactBuildError::Host(FsError::Io {
                operation: FsOperation::Read,
                source,
            })
        })?;
        let len = usize::try_from(len).map_err(|_| ArtifactBuildError::Overflow)?;
        if len > maximum {
            return Err(if cache {
                ArtifactBuildError::InvalidCache("stored object exceeds its byte bound")
            } else {
                ArtifactBuildError::Budget {
                    kind: "encoded",
                    required: len,
                    limit: maximum,
                }
            });
        }
        let lease = self.reserve_work(len)?;
        let bytes = file.read_to_end_bounded(len as u64).map_err(|_| {
            ArtifactBuildError::InvalidCache(
                "stored object could not be captured within its byte bound",
            )
        })?;
        Ok(format::Encoded {
            bytes,
            _lease: lease,
        })
    }

    fn publish_object(
        &self,
        name: &RelativeComponent,
        bytes: &[u8],
        digest: ArtifactContentDigest,
    ) -> Result<(), ArtifactBuildError> {
        match self.root.open_child_file(name) {
            Ok(existing) => {
                existing
                    .verify_digest(digest.content(), digest.length())
                    .map_err(|_| {
                        ArtifactBuildError::InvalidCache(
                            "existing content-addressed object is corrupt",
                        )
                    })?;
            }
            Err(error) if missing(&error) => {
                let receipt = self.publish(name, bytes, ExpectedTarget::Missing)?;
                if receipt.verified_content_digest() != Some(digest.content()) {
                    return Err(ArtifactBuildError::InvalidCache(
                        "new content-addressed object did not verify",
                    ));
                }
            }
            Err(_) => {
                return Err(ArtifactBuildError::InvalidCache(
                    "content-addressed object is inaccessible",
                ));
            }
        }
        Ok(())
    }

    fn publish(
        &self,
        target: &RelativeComponent,
        bytes: &[u8],
        expected: ExpectedTarget,
    ) -> Result<ReplaceReceipt, ArtifactBuildError> {
        self.root.preflight_replace(expected)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ArtifactBuildError::Overflow)?
            .as_nanos();
        let name = component(&format!(
            "artifact-tmp-{}-{stamp}-{sequence}",
            std::process::id()
        ))?;
        let mut temporary = self.root.create_temp(&name)?;
        if let Err(source) = temporary.write_all(bytes) {
            return Err(TemporaryFileFailure::with_cleanup(
                FsError::Io {
                    operation: FsOperation::Write,
                    source,
                },
                temporary.into_cleanup(),
            )
            .into());
        }
        if let Err(error) = temporary.sync() {
            return Err(TemporaryFileFailure::with_cleanup(error, temporary.into_cleanup()).into());
        }
        Ok(self.root.replace_temp(temporary, target, expected)?)
    }
}

fn invoke_control(
    control: &mut impl FnMut(ArtifactPublicationPoint) -> Result<(), ArtifactBuildError>,
    point: ArtifactPublicationPoint,
) -> Result<(), ArtifactBuildError> {
    catch_unwind(AssertUnwindSafe(|| control(point))).unwrap_or(Err(ArtifactBuildError::Panicked))
}

struct IndexSnapshot {
    index: StoreIndex,
    file: Option<FileCapability>,
    digest: Option<ArtifactContentDigest>,
    _work: Option<ArtifactBuildLease>,
}

fn retained_size(manifest: &GroupManifest) -> Result<usize, ArtifactBuildError> {
    let mut total = add(size_of::<VerifiedGroupInner>(), 2 * size_of::<usize>())?;
    total = add(total, format::manifest_allocation(manifest)?)?;
    total = add(
        total,
        mul(
            manifest.members.len(),
            size_of::<Arc<[u8]>>() + 2 * size_of::<usize>(),
        )?,
    )?;
    for member in &manifest.members {
        total = add(
            total,
            usize::try_from(member.digest.length()).map_err(|_| ArtifactBuildError::Overflow)?,
        )?;
    }
    Ok(total)
}

fn component(name: &str) -> Result<RelativeComponent, ArtifactBuildError> {
    RelativeComponent::new(name).map_err(|error| ArtifactBuildError::Host(error.into()))
}

fn blob_name(digest: ArtifactContentDigest) -> Result<RelativeComponent, ArtifactBuildError> {
    component(&format!("blob-{}-{}", digest.length(), digest.hex()))
}

fn group_name(digest: ArtifactContentDigest) -> Result<RelativeComponent, ArtifactBuildError> {
    component(&format!("group-{}-{}.json", digest.length(), digest.hex()))
}

fn missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(all(test, any(windows, target_os = "linux")))]
#[path = "store_tests.rs"]
mod tests;
