//! No-replace publication of one captured local release. Activation remains Session-owned.

use std::{collections::BTreeMap, fmt, io::Write};

use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, DirectoryEnumerationLimits,
    DirectoryPublicationBeginError, DirectoryPublicationOutcome, DirectoryPublicationReceipt,
    ExpectedTarget, FileCapability, FileIdentity, FileKind, FileSyncReceipt, FsError,
    RelativeComponent, RelativePath, ReplaceReceipt, TemporaryFileCleanup,
};

use super::{
    EditorSourceToolReleaseMemberKind, PreparedEditorSourceToolRelease, ReleaseCaptureBudget,
    ReleaseMetadata, manifest::MANIFEST_NAME,
};

mod receipt;
pub use receipt::EditorSourceToolInstallationReceipt;
use receipt::{MAX_RECEIPT_BYTES, RECEIPT_NAME};

/// Retains the exact candidate shown in a preview. Dropping or cancelling it performs no writes.
pub struct EditorSourceToolInstallationPlan {
    parent: DirectoryCapability,
    destination: RelativeComponent,
    release: PreparedEditorSourceToolRelease,
}

/// Observed publication and live created-file provenance, not executable approval.
///
/// The original created handles remain alive until each matching read handle is acquired, so
/// native identity reuse cannot establish ownership. Closing this value relinquishes those live
/// facts. The persisted receipt alone cannot reconstruct them or authorize deletion after Open.
pub struct EditorSourceToolInstallation {
    parent: DirectoryCapability,
    destination: RelativeComponent,
    directory: DirectoryCapability,
    metadata: ReleaseMetadata,
    receipt: EditorSourceToolInstallationReceipt,
    receipt_digest: ContentDigest,
    publication: DirectoryPublicationReceipt,
    receipt_publication: ReplaceReceipt,
    members: Vec<EditorSourceToolInstalledMember>,
}

/// One created member with continuous live-object provenance and its last observed content.
pub struct EditorSourceToolInstalledMember {
    path: String,
    kind: Option<EditorSourceToolReleaseMemberKind>,
    digest: ContentDigest,
    sync: FileSyncReceipt,
    file: FileCapability,
}

impl fmt::Debug for EditorSourceToolInstalledMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorSourceToolInstalledMember")
            .field("path", &self.path)
            .field("kind", &self.kind)
            .field("digest", &self.digest)
            .field("sync", &self.sync)
            .finish_non_exhaustive()
    }
}

impl EditorSourceToolInstalledMember {
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Generated manifest/receipt members return None; declared payloads retain their role.
    #[must_use]
    pub const fn kind(&self) -> Option<EditorSourceToolReleaseMemberKind> {
        self.kind
    }
    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
    #[must_use]
    pub const fn sync_receipt(&self) -> FileSyncReceipt {
        self.sync
    }

    pub(in crate::project_host::runtime::editor::source_tools) fn file(&self) -> &FileCapability {
        &self.file
    }
}

/// Code-free inspection retaining the exact live directory and ready-receipt byte observation.
///
/// The retained directory preserves inspected-object continuity, not created-file provenance or
/// deletion authority. Captured release bytes remain independent of later namespace changes.
pub struct EditorSourceToolInstallationInspection {
    directory: DirectoryCapability,
    receipt: EditorSourceToolInstallationReceipt,
    receipt_digest: ContentDigest,
    release: PreparedEditorSourceToolRelease,
}

/// An unready candidate retains actual namespace facts and every acquired cleanup handle.
///
/// Cleanup handles here refer only to newly created, unaccepted candidate members. They must not
/// become an installed-content deletion path; adoption and changed-content policy are separate.
#[derive(Debug)]
pub struct EditorSourceToolInstallationOrphan {
    publication: DirectoryPublicationOutcome,
    directory: Option<DirectoryCapability>,
    files: Vec<TemporaryFileCleanup>,
    readers: Vec<EditorSourceToolInstalledMember>,
}

/// Filesystem publication and later content observations are separate facts.
#[must_use = "observe publication, retained cleanup and any post-publication reconciliation"]
pub enum EditorSourceToolInstallationOutcome {
    Published(EditorSourceToolInstallation),
    PublishedUnverified {
        installation: EditorSourceToolInstallation,
        error: EditorSourceToolInstallationError,
    },
    Failed {
        plan: EditorSourceToolInstallationPlan,
        error: EditorSourceToolInstallationError,
        orphan: Option<EditorSourceToolInstallationOrphan>,
    },
    /// An empty candidate directory was published, but no complete ready receipt was committed.
    Incomplete {
        plan: EditorSourceToolInstallationPlan,
        error: EditorSourceToolInstallationError,
        orphan: EditorSourceToolInstallationOrphan,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum EditorSourceToolInstallationError {
    #[error("the installation parent is read-only")]
    ReadOnly,
    #[error("the release uses a reserved installation receipt path")]
    ReservedPath,
    #[error("installation receipt preparation failed: {0}")]
    Receipt(&'static str),
    #[error("directory publication could not begin")]
    Begin(#[source] DirectoryPublicationBeginError),
    #[error("the installation member could not be prepared: {member}")]
    Member {
        member: String,
        #[source]
        source: FsError,
    },
    #[error("the installation member could not be written: {member}")]
    Write {
        member: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the installation member changed: {member}")]
    Changed { member: String },
    #[error("directory publication did not complete")]
    Publication,
    #[error("the persisted installation could not be inspected")]
    Inspection(#[source] super::EditorSourceToolReleaseError),
}

impl EditorSourceToolInstallationPlan {
    /// Captures destination authority without reserving a name, executing code or writing a file.
    ///
    /// This publishes a release artifact directory only. It does not select an installed Package
    /// graph, satisfy required dependencies, enable a tool or adopt metadata into a project.
    pub fn new(
        parent: DirectoryCapability,
        destination: RelativeComponent,
        release: PreparedEditorSourceToolRelease,
    ) -> Result<Self, EditorSourceToolInstallationError> {
        if parent.options().rights() != CapabilityRights::ReadWrite {
            return Err(EditorSourceToolInstallationError::ReadOnly);
        }
        if release.members().iter().any(|member| {
            let path = member.path().to_ascii_lowercase();
            path == RECEIPT_NAME || path.starts_with(&format!("{RECEIPT_NAME}/"))
        }) {
            return Err(EditorSourceToolInstallationError::ReservedPath);
        }
        Ok(Self {
            parent,
            destination,
            release,
        })
    }

    /// The same immutable input that confirmation will publish; no refreshed source is substituted.
    #[must_use]
    pub fn release(&self) -> &PreparedEditorSourceToolRelease {
        &self.release
    }

    #[must_use]
    pub fn destination(&self) -> &RelativeComponent {
        &self.destination
    }

    #[must_use]
    pub fn parent_identity(&self) -> FileIdentity {
        self.parent.identity()
    }

    /// Cancels before staging and returns the unchanged captured input.
    #[must_use]
    pub fn cancel(self) -> PreparedEditorSourceToolRelease {
        self.release
    }

    /// Explicitly confirms publication of this candidate under a previously absent destination.
    ///
    /// Run in owned I/O preparation, not during paint. Each file is exclusively created and a
    /// synchronization receipt records its actual tier.
    /// The empty candidate directory is published first. Payloads are then written there and a
    /// no-replace ready receipt is committed last; this is not an atomic directory-tree switch.
    /// Incomplete retains an unready directory and custody. After the ready receipt publishes,
    /// admission refusal is PublishedUnverified, never a claim that nothing was published.
    pub fn publish(self) -> EditorSourceToolInstallationOutcome {
        self.publish_with(|_| Ok(()), |_, _| Ok(()))
    }

    fn publish_with(
        self,
        before_publish: impl FnOnce(
            &DirectoryCapability,
        ) -> Result<(), EditorSourceToolInstallationError>,
        after_publish: impl FnOnce(
            &DirectoryCapability,
            &DirectoryCapability,
        ) -> Result<(), EditorSourceToolInstallationError>,
    ) -> EditorSourceToolInstallationOutcome {
        let prepared = (|| {
            let receipt =
                EditorSourceToolInstallationReceipt::issue(self.release.manifest_digest())
                    .map_err(EditorSourceToolInstallationError::Receipt)?;
            let bytes = receipt
                .encode()
                .map_err(EditorSourceToolInstallationError::Receipt)?;
            let staging = staging_name()?;
            Ok::<_, EditorSourceToolInstallationError>((receipt, bytes, staging))
        })();
        let (receipt, receipt_bytes, staging_name) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => return failed(self, error, None),
        };
        let transaction = match self
            .parent
            .begin_directory_publication(&self.destination, &staging_name)
        {
            Ok(transaction) => transaction,
            Err(error) => {
                return failed(self, EditorSourceToolInstallationError::Begin(error), None);
            }
        };
        let staging = transaction.staging_directory();
        // A nonempty directory cannot be renamed while continuously retaining its member
        // handles on the qualified Windows filesystem. Publish an EMPTY unready candidate first.
        let publication = match transaction.publish() {
            DirectoryPublicationOutcome::Published(publication) => publication,
            outcome => {
                return failed(
                    self,
                    EditorSourceToolInstallationError::Publication,
                    Some(EditorSourceToolInstallationOrphan {
                        publication: outcome,
                        directory: Some(staging),
                        files: Vec::new(),
                        readers: Vec::new(),
                    }),
                );
            }
        };
        let mut staged = StagedMembers::default();
        let stage_result = stage_release(&staging, &self.release, &mut staged)
            .and_then(|()| before_publish(&staging))
            .and_then(|()| {
                verify_destination(&self.parent, &self.destination, &staging, &staged.readers)
            });
        if let Err(error) = stage_result {
            return incomplete(self, error, publication, staging, staged);
        }
        let receipt_publication = match commit_receipt(&staging, &receipt_bytes, &mut staged) {
            Ok(receipt_publication) => receipt_publication,
            Err(error) => return incomplete(self, error, publication, staging, staged),
        };
        let receipt_digest = ContentDigest::of_bytes(&receipt_bytes);
        let verification = after_publish(&self.parent, &staging).and_then(|()| {
            if receipt_publication.verified_content_digest() != Some(receipt_digest) {
                return Err(EditorSourceToolInstallationError::Changed {
                    member: RECEIPT_NAME.to_owned(),
                });
            }
            verify_destination(&self.parent, &self.destination, &staging, &staged.readers)
        });
        // Exact read handles have held every created object continuously before writers close.
        staged.writers.clear();
        let installation = EditorSourceToolInstallation {
            parent: self.parent,
            destination: self.destination,
            directory: staging,
            metadata: self.release.metadata,
            receipt,
            receipt_digest,
            publication,
            receipt_publication,
            members: staged.readers,
        };
        match verification {
            Ok(()) => EditorSourceToolInstallationOutcome::Published(installation),
            Err(error) => EditorSourceToolInstallationOutcome::PublishedUnverified {
                installation,
                error,
            },
        }
    }
}

impl EditorSourceToolInstallation {
    #[must_use]
    pub fn package(&self) -> &str {
        &self.metadata.package
    }
    #[must_use]
    pub fn release(&self) -> &str {
        &self.metadata.release
    }
    #[must_use]
    pub fn receipt(&self) -> &EditorSourceToolInstallationReceipt {
        &self.receipt
    }
    #[must_use]
    pub const fn receipt_digest(&self) -> ContentDigest {
        self.receipt_digest
    }
    #[must_use]
    pub fn publication(&self) -> &DirectoryPublicationReceipt {
        &self.publication
    }
    #[must_use]
    pub fn receipt_publication(&self) -> &ReplaceReceipt {
        &self.receipt_publication
    }
    /// Borrows exact directory authority, not an authorization-checked ambient path.
    #[must_use]
    pub fn directory(&self) -> &DirectoryCapability {
        &self.directory
    }
    #[must_use]
    pub fn members(&self) -> &[EditorSourceToolInstalledMember] {
        &self.members
    }
    /// Verifies current names, live objects and bytes. A successful observation is not a write lock.
    pub fn verify(&self) -> Result<(), EditorSourceToolInstallationError> {
        verify_destination(
            &self.parent,
            &self.destination,
            &self.directory,
            &self.members,
        )
    }
}

impl EditorSourceToolInstallationInspection {
    /// Reopens and captures a receipt-selected declared release without recovering deletion rights.
    pub fn read(
        root: &DirectoryCapability,
        expected_receipt: ContentDigest,
    ) -> Result<Self, EditorSourceToolInstallationError> {
        let maximum_bytes = usize::try_from(super::MAX_RELEASE_CAPTURE_BYTES + MAX_RECEIPT_BYTES)
            .map_err(|_| {
            EditorSourceToolInstallationError::Inspection(
                super::EditorSourceToolReleaseError::CaptureBudget,
            )
        })?;
        Self::read_with_budget(
            root,
            expected_receipt,
            &mut ReleaseCaptureBudget::new(maximum_bytes),
        )
    }

    /// Shares one byte-work allowance across receipt, manifest and all declared payload reads.
    pub(in crate::project_host::runtime::editor::source_tools) fn read_with_budget(
        root: &DirectoryCapability,
        expected_receipt: ContentDigest,
        budget: &mut ReleaseCaptureBudget,
    ) -> Result<Self, EditorSourceToolInstallationError> {
        super::require_strict_source(root)
            .map_err(EditorSourceToolInstallationError::Inspection)?;
        if expected_receipt.length() == 0 || expected_receipt.length() > MAX_RECEIPT_BYTES {
            return Err(EditorSourceToolInstallationError::Receipt(
                "invalid receipt byte length",
            ));
        }
        let directory = root.share();
        let bytes = super::capture_member(&directory, RECEIPT_NAME, expected_receipt, budget)
            .map_err(EditorSourceToolInstallationError::Inspection)?;
        let receipt = EditorSourceToolInstallationReceipt::decode(&bytes)
            .map_err(EditorSourceToolInstallationError::Receipt)?;
        let release = PreparedEditorSourceToolRelease::capture_with_budget(
            &directory,
            receipt.manifest_digest(),
            budget,
        )
        .map_err(EditorSourceToolInstallationError::Inspection)?;
        verify_inventory(
            &directory,
            release
                .members()
                .iter()
                .map(|member| member.path())
                .chain([MANIFEST_NAME, RECEIPT_NAME]),
        )?;
        Ok(Self {
            directory,
            receipt,
            receipt_digest: expected_receipt,
            release,
        })
    }
    #[must_use]
    pub fn receipt(&self) -> &EditorSourceToolInstallationReceipt {
        &self.receipt
    }
    /// Digest of the exact inspected ready-receipt bytes, without re-encoding the parsed facts.
    #[must_use]
    pub const fn receipt_digest(&self) -> ContentDigest {
        self.receipt_digest
    }
    /// Identity of the continuously retained inspected directory, not a durable trust identifier.
    #[must_use]
    pub fn directory_identity(&self) -> FileIdentity {
        self.directory.identity()
    }
    pub(in super::super) fn directory(&self) -> &DirectoryCapability {
        &self.directory
    }
    #[must_use]
    pub fn release(&self) -> &PreparedEditorSourceToolRelease {
        &self.release
    }
}

impl fmt::Debug for EditorSourceToolInstallationInspection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorSourceToolInstallationInspection")
            .field("package", &self.release.package())
            .field("installation", self.receipt.installation_id())
            .field("directory_identity", &self.directory_identity())
            .field("receipt_digest", &self.receipt_digest)
            .field("manifest", &self.release.manifest_digest())
            .finish_non_exhaustive()
    }
}

impl EditorSourceToolInstallationOrphan {
    #[must_use]
    pub fn publication(&self) -> &DirectoryPublicationOutcome {
        &self.publication
    }
    #[must_use]
    pub fn pending_files(&self) -> usize {
        self.files.len()
    }
    /// Last observed created members. These facts do not prove unchanged content for deletion.
    #[must_use]
    pub fn members(&self) -> &[EditorSourceToolInstalledMember] {
        &self.readers
    }
    #[must_use]
    pub fn directory(&self) -> Option<&DirectoryCapability> {
        self.directory.as_ref()
    }
}

#[derive(Default)]
struct StagedMembers {
    writers: Vec<TemporaryFileCleanup>,
    readers: Vec<EditorSourceToolInstalledMember>,
}

fn failed(
    plan: EditorSourceToolInstallationPlan,
    error: EditorSourceToolInstallationError,
    orphan: Option<EditorSourceToolInstallationOrphan>,
) -> EditorSourceToolInstallationOutcome {
    EditorSourceToolInstallationOutcome::Failed {
        plan,
        error,
        orphan,
    }
}

fn incomplete(
    plan: EditorSourceToolInstallationPlan,
    error: EditorSourceToolInstallationError,
    publication: DirectoryPublicationReceipt,
    directory: DirectoryCapability,
    staged: StagedMembers,
) -> EditorSourceToolInstallationOutcome {
    EditorSourceToolInstallationOutcome::Incomplete {
        plan,
        error,
        orphan: EditorSourceToolInstallationOrphan {
            publication: DirectoryPublicationOutcome::Published(publication),
            directory: Some(directory),
            files: staged.writers,
            readers: staged.readers,
        },
    }
}

fn stage_release(
    root: &DirectoryCapability,
    release: &PreparedEditorSourceToolRelease,
    staged: &mut StagedMembers,
) -> Result<(), EditorSourceToolInstallationError> {
    let mut directories = BTreeMap::new();
    for (path, bytes, digest, kind) in std::iter::once((
        MANIFEST_NAME,
        release.manifest_bytes(),
        release.manifest_digest(),
        None,
    ))
    .chain(release.members().iter().map(|member| {
        (
            member.path(),
            member.bytes(),
            member.digest(),
            Some(member.kind()),
        )
    })) {
        let mut parent = root.share();
        let parts: Vec<_> = path.split('/').collect();
        let mut prefix = String::new();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if let Some(directory) = directories.get(&prefix) {
                parent = DirectoryCapability::share(directory);
            } else {
                let component = RelativeComponent::new(part)
                    .map_err(|source| member_error(path, source.into()))?;
                let directory = parent
                    .create_child_directory(&component)
                    .map_err(|source| member_error(path, source))?;
                parent = directory.share();
                directories.insert(prefix.clone(), directory);
            }
        }
        let leaf = RelativeComponent::new(parts[parts.len() - 1])
            .map_err(|source| member_error(path, source.into()))?;
        let mut file = match parent.create_temp(&leaf) {
            Ok(file) => file,
            Err(failure) => {
                let (source, cleanup) = failure.into_parts();
                staged.writers.extend(cleanup);
                return Err(member_error(path, source));
            }
        };
        let identity = file.identity();
        let result = file
            .write_all(bytes)
            .map_err(|source| EditorSourceToolInstallationError::Write {
                member: path.to_owned(),
                source,
            })
            .and_then(|()| file.sync().map_err(|source| member_error(path, source)));
        staged.writers.push(file.into_cleanup());
        let sync = result?;
        let reader = root
            .open_file_exact(
                &RelativePath::new(path).map_err(|source| member_error(path, source.into()))?,
            )
            .map_err(|source| member_error(path, source))?;
        if reader.identity() != identity
            || reader
                .digest(digest.length())
                .map_err(|source| member_error(path, source))?
                != digest
        {
            return Err(EditorSourceToolInstallationError::Changed {
                member: path.to_owned(),
            });
        }
        staged.readers.push(EditorSourceToolInstalledMember {
            path: path.to_owned(),
            kind,
            digest,
            sync,
            file: reader,
        });
    }
    Ok(())
}

fn commit_receipt(
    root: &DirectoryCapability,
    bytes: &[u8],
    staged: &mut StagedMembers,
) -> Result<ReplaceReceipt, EditorSourceToolInstallationError> {
    let name = staging_name()?;
    let path = name
        .to_str()
        .ok_or(EditorSourceToolInstallationError::Receipt(
            "invalid receipt staging name",
        ))?;
    let mut temporary = match root.create_temp(&name) {
        Ok(temporary) => temporary,
        Err(failure) => {
            let (source, cleanup) = failure.into_parts();
            staged.writers.extend(cleanup);
            return Err(member_error(RECEIPT_NAME, source));
        }
    };
    let prepared = (|| {
        temporary
            .write_all(bytes)
            .map_err(|source| EditorSourceToolInstallationError::Write {
                member: RECEIPT_NAME.to_owned(),
                source,
            })?;
        let sync = temporary
            .sync()
            .map_err(|source| member_error(RECEIPT_NAME, source))?;
        let reader = root
            .open_file_exact(
                &RelativePath::new(path)
                    .map_err(|source| member_error(RECEIPT_NAME, source.into()))?,
            )
            .map_err(|source| member_error(RECEIPT_NAME, source))?;
        if reader.identity() != temporary.identity()
            || reader
                .digest(bytes.len() as u64)
                .map_err(|source| member_error(RECEIPT_NAME, source))?
                != ContentDigest::of_bytes(bytes)
        {
            return Err(EditorSourceToolInstallationError::Changed {
                member: RECEIPT_NAME.to_owned(),
            });
        }
        Ok::<_, EditorSourceToolInstallationError>((sync, reader))
    })();
    let (sync, reader) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            staged.writers.push(temporary.into_cleanup());
            return Err(error);
        }
    };
    let target =
        RelativeComponent::new(RECEIPT_NAME).expect("the generated receipt has a valid fixed name");
    let publication = match root.replace_temp(temporary, &target, ExpectedTarget::Missing) {
        Ok(publication) => publication,
        Err(failure) => {
            let (source, cleanup) = failure.into_parts();
            staged.writers.extend(cleanup);
            return Err(member_error(RECEIPT_NAME, source));
        }
    };
    staged.readers.push(EditorSourceToolInstalledMember {
        path: RECEIPT_NAME.to_owned(),
        kind: None,
        digest: ContentDigest::of_bytes(bytes),
        sync,
        file: reader,
    });
    Ok(publication)
}

fn verify_destination(
    parent: &DirectoryCapability,
    destination: &RelativeComponent,
    original: &DirectoryCapability,
    members: &[EditorSourceToolInstalledMember],
) -> Result<(), EditorSourceToolInstallationError> {
    let current = parent
        .open_directory_exact(
            &RelativePath::from_components([destination.clone()])
                .map_err(|source| member_error("installation directory", source.into()))?,
        )
        .map_err(|source| member_error("installation directory", source))?;
    if current.identity() != original.identity() {
        return Err(EditorSourceToolInstallationError::Changed {
            member: "installation directory".to_owned(),
        });
    }
    verify_members(&current, members)
}

fn verify_members(
    root: &DirectoryCapability,
    members: &[EditorSourceToolInstalledMember],
) -> Result<(), EditorSourceToolInstallationError> {
    for member in members {
        let current = root
            .open_file_exact(
                &RelativePath::new(&member.path)
                    .map_err(|source| member_error(&member.path, source.into()))?,
            )
            .map_err(|source| member_error(&member.path, source))?;
        if current.identity() != member.file.identity()
            || current
                .digest(member.digest.length())
                .map_err(|source| member_error(&member.path, source))?
                != member.digest
        {
            return Err(EditorSourceToolInstallationError::Changed {
                member: member.path.clone(),
            });
        }
    }
    verify_inventory(root, members.iter().map(|member| member.path.as_str()))
}

fn verify_inventory<'a>(
    root: &DirectoryCapability,
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<(), EditorSourceToolInstallationError> {
    let mut expected = BTreeMap::new();
    for path in paths {
        expected.insert(path.to_owned(), FileKind::Regular);
        for (separator, _) in path.match_indices('/') {
            expected.insert(path[..separator].to_owned(), FileKind::Directory);
        }
    }
    let limit = expected.len();
    let mut consumed = 0;
    let mut pending = vec![(root.share(), String::new())];
    while let Some((directory, prefix)) = pending.pop() {
        let mut scan = directory
            .begin_child_enumeration(DirectoryEnumerationLimits::new(
                limit.saturating_sub(consumed),
                512,
                0,
            ))
            .map_err(|source| member_error("installation tree", source))?;
        loop {
            let batch = scan
                .next_batch(64)
                .map_err(|source| member_error("installation tree", source))?;
            if !batch.rejected_entry_facts().is_empty() {
                return Err(EditorSourceToolInstallationError::Changed {
                    member: "installation tree".to_owned(),
                });
            }
            for entry in batch.entries() {
                consumed += 1;
                let name = entry.name().to_str().ok_or_else(|| {
                    EditorSourceToolInstallationError::Changed {
                        member: "installation tree".to_owned(),
                    }
                })?;
                let path = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                };
                if expected.remove(&path) != Some(entry.kind()) {
                    return Err(EditorSourceToolInstallationError::Changed { member: path });
                }
                if entry.kind() == FileKind::Directory {
                    let child = root
                        .open_directory_exact(
                            &RelativePath::new(&path)
                                .map_err(|source| member_error(&path, source.into()))?,
                        )
                        .map_err(|source| member_error(&path, source))?;
                    if Some(child.identity()) != entry.identity() {
                        return Err(EditorSourceToolInstallationError::Changed { member: path });
                    }
                    pending.push((child, path));
                }
            }
            if batch.is_complete() {
                break;
            }
        }
    }
    if expected.is_empty() {
        Ok(())
    } else {
        Err(EditorSourceToolInstallationError::Changed {
            member: "installation tree".to_owned(),
        })
    }
}

fn member_error(path: &str, source: FsError) -> EditorSourceToolInstallationError {
    EditorSourceToolInstallationError::Member {
        member: path.to_owned(),
        source,
    }
}

fn staging_name() -> Result<RelativeComponent, EditorSourceToolInstallationError> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce)
        .map_err(|_| EditorSourceToolInstallationError::Receipt("staging entropy unavailable"))?;
    let nonce: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
    RelativeComponent::new(format!(".nara-tool-stage-{nonce}"))
        .map_err(|_| EditorSourceToolInstallationError::Receipt("invalid staging name"))
}

impl fmt::Debug for EditorSourceToolInstallationPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorSourceToolInstallationPlan")
            .field("destination", &self.destination)
            .field("release", &self.release)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for EditorSourceToolInstallation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorSourceToolInstallation")
            .field("package", &self.metadata.package)
            .field("receipt", &self.receipt)
            .field("publication", &self.publication)
            .field("members", &self.members.len())
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for EditorSourceToolInstallationOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Published(value) => f.debug_tuple("Published").field(value).finish(),
            Self::PublishedUnverified {
                installation,
                error,
            } => f
                .debug_struct("PublishedUnverified")
                .field("installation", installation)
                .field("error", error)
                .finish(),
            Self::Failed { error, orphan, .. } => f
                .debug_struct("Failed")
                .field("error", error)
                .field("orphan", orphan)
                .finish_non_exhaustive(),
            Self::Incomplete { error, orphan, .. } => f
                .debug_struct("Incomplete")
                .field("error", error)
                .field("orphan", orphan)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(all(
    test,
    target_os = "windows",
    target_arch = "x86_64",
    target_env = "msvc"
))]
mod tests;
