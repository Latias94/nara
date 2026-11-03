//! Actual-file custody for a fixed code-free recovery package and Pending manifest.
//!
//! This owner publishes recovery data before Pending and reads that data in a fresh process.
//! It never publishes source/bundle replacements, executes an upgrader, admits a ready project
//! or manufactures a replacement receipt from a read. Ordinary project admission still refuses
//! Pending. The enclosing operation owns complete membership and all subsequent installation.

use std::{io::ErrorKind, mem::size_of, sync::Arc};

use nara_core::{ByteLimit, SourceUniverseId};
use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveProjectSettings, ProjectAuthoringRecovery,
    ProjectAuthoringRecoveryDigest, ProjectAuthoringState, ProjectManifest, ProjectPath,
};

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError, PROJECT_MANIFEST_PATH,
};
use crate::project_host::file_publication::{
    FilePublicationBinding, FilePublicationEvidence, FilePublicationFailureStage,
    FilePublicationHost, FilePublicationOutcome, FilePublicationRejection,
};

/// Immutable data projection only. Payload grammar, Schema, semantic generation and final live
/// installation remain with each concrete operation. This is not an execution policy hook.
pub(super) trait RecoveryPackageData {
    fn encoded_bytes(&self) -> &[u8];
    fn digest(&self) -> ProjectAuthoringRecoveryDigest;
    fn source_universe(&self) -> SourceUniverseId;
    fn old_ready_manifest(&self) -> &[u8];
    fn final_ready_manifest(&self) -> &[u8];
    fn protected_paths(&self) -> impl Iterator<Item = &str>;
}

// Exact-path verification and digest hashing each use up to 64 KiB of native/stack scratch.
// They may overlap in a file-publication call. All retained file observations use this root's
// capability session; FileIdentity is never recovered from the serialized package.
const NATIVE_SCRATCH: usize = 2 * 65_536;
pub(super) const HANDLE_PEAK: usize = 12;
// Ready leaf, recovery leaf and parent, plus Pending publisher leaf/cleanup capacity.
pub(super) const RETAINED_HANDLE_CAPACITY: usize = 5;
// After confirmed Pending, its manifest, recovery file and recovery parent remain live.
pub(super) const PENDING_HANDLE_CAPACITY: usize = 3;
// The inventory owner uses the same bound for a directory's shared inner and Arc counters.
const NATIVE_PARENT_BYTES: usize = size_of::<FileCapability>() + 2 * size_of::<usize>();

pub(super) struct RecoveryStorageHost {
    root: DirectoryCapability,
    ready_file: FileCapability,
    ready_digest: ContentDigest,
    ready_bytes: Vec<u8>,
}

/// Refusal returns all incumbent custody; only this attempt's discarded candidate is refunded.
pub(super) struct RecoveryStoragePrepareFailure<P> {
    pub(super) host: RecoveryStorageHost,
    pub(super) recovery: Arc<P>,
    pub(super) path: ProjectPath,
    pub(super) error: RecoveryStorageError,
}

impl<P> std::fmt::Debug for RecoveryStoragePrepareFailure<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryStoragePrepareFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl RecoveryStorageHost {
    #[cfg(test)]
    pub(super) fn test_ready_observation(&self) -> (&FileCapability, &[u8]) {
        (&self.ready_file, &self.ready_bytes)
    }

    pub(super) fn capture(
        root: DirectoryCapability,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<Self, RecoveryStorageError> {
        with_candidate_storage(budget, |budget| {
            budget.charge_source_bytes(0, 0, 0, size_of::<Self>())?;
            let (ready_file, ready_bytes, ready_digest, _) = read_file(
                &root,
                PROJECT_MANIFEST_PATH,
                manifest_limit(),
                RecoveryStorageSource::Manifest,
                budget,
            )?;
            with_manifest(&ready_bytes, budget, |manifest, _| {
                if !matches!(manifest.authoring.state, ProjectAuthoringState::Ready) {
                    return Err(RecoveryStorageError::ReadyRequired);
                }
                Ok(())
            })?;
            verify_named(
                &root,
                PROJECT_MANIFEST_PATH,
                &ready_file,
                ready_digest,
                RecoveryStorageSource::Manifest,
                budget,
            )?;
            Ok(Self {
                root,
                ready_file,
                ready_digest,
                ready_bytes,
            })
        })
    }

    /// All fallible parsing/serialization precedes the first package write. The package is
    /// already typed and verified by its data owner; this method does not reinterpret its AST.
    pub(super) fn prepare<P: RecoveryPackageData>(
        self,
        recovery: Arc<P>,
        path: ProjectPath,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<PreparedRecoveryStorage<P>, RecoveryStoragePrepareFailure<P>> {
        let result = with_candidate_storage(budget, |budget| {
            self.prepare_candidate(recovery.as_ref(), &path, budget)
        });
        match result {
            Ok((reference, pending_bytes, publisher, recovery_parent)) => {
                Ok(PreparedRecoveryStorage {
                    host: self,
                    recovery,
                    reference,
                    pending_bytes,
                    publisher,
                    recovery_parent,
                })
            }
            Err(error) => Err(RecoveryStoragePrepareFailure {
                host: self,
                recovery,
                path,
                error,
            }),
        }
    }

    fn prepare_candidate<P: RecoveryPackageData>(
        &self,
        recovery: &P,
        path: &ProjectPath,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<
        (
            ProjectAuthoringRecovery,
            Vec<u8>,
            FilePublicationHost,
            DirectoryCapability,
        ),
        RecoveryStorageError,
    > {
        budget.charge_source_bytes(
            0,
            0,
            add(self.ready_bytes.len(), recovery.old_ready_manifest().len())?,
            0,
        )?;
        if recovery.old_ready_manifest() != self.ready_bytes {
            return Err(RecoveryStorageError::OriginalManifestMismatch);
        }
        budget.charge_source_bytes(0, 0, path.as_str().len(), path.as_str().len())?;
        let reference = ProjectAuthoringRecovery::new(path.clone(), recovery.digest())
            .map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
        validate_pending_bytes(
            &reference,
            path.as_str(),
            Some(recovery.encoded_bytes()),
            ByteLimit::new(recovery.encoded_bytes().len().max(1))
                .ok_or(RecoveryStorageError::AccountingOverflow)?,
            budget,
        )?;
        validate_storage_path(reference.recovery_path().as_str(), recovery, budget)?;
        let pending_bytes = with_manifest(&self.ready_bytes, budget, |mut manifest, budget| {
            if !matches!(manifest.authoring.state, ProjectAuthoringState::Ready)
                || manifest.project.source_universe != recovery.source_universe()
            {
                return Err(RecoveryStorageError::OriginalManifestMismatch);
            }
            validate_configuration(&manifest, recovery, budget)?;
            // Admit the reference clone before replacing the one modeled field. Project's
            // canonical writer owns all manifest syntax and output validation.
            budget.charge_source_bytes(0, 0, reference.recovery_path().as_str().len(), 0)?;
            with_temporary(reference.recovery_path().as_str().len(), budget, |budget| {
                manifest.authoring.state = ProjectAuthoringState::Pending(reference.clone());
                let result = encode_manifest(&manifest, budget);
                drop(manifest);
                result
            })
        })?;
        let (publisher, recovery_parent) =
            with_path_scratch(reference.recovery_path().as_str(), budget, |budget| {
                verify_named(
                    &self.root,
                    PROJECT_MANIFEST_PATH,
                    &self.ready_file,
                    self.ready_digest,
                    RecoveryStorageSource::Manifest,
                    budget,
                )?;
                // Retain a parent and a possible failed-cleanup leaf before acquiring either.
                // A confirmed package transfers the cleanup slot to its accepted live file.
                budget.charge_source_bytes(0, 0, 0, NATIVE_PARENT_BYTES)?;
                budget.retain_open_handles(2)?;
                budget.admit_handle_peak(HANDLE_PEAK)?;
                let parent = exact_parent(&self.root, reference.recovery_path().as_str())?;
                // bind_missing alone intentionally does not test for existence. Refuse an existing
                // leaf now and still keep its exact Missing expectation for a competing later write.
                let relative = RelativePath::new(reference.recovery_path().as_str())
                    .map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
                let leaf = relative
                    .components()
                    .last()
                    .expect("a validated path has a leaf");
                match parent.open_child_file(leaf) {
                    Ok(_) => return Err(RecoveryStorageError::RecoveryAlreadyExists),
                    Err(error) if missing(&error) => {}
                    Err(error) => {
                        return Err(RecoveryStorageError::Filesystem {
                            kind: RecoveryStorageSource::Recovery,
                            error,
                        });
                    }
                }
                budget.charge_source_bytes(0, 0, 0, size_of::<FilePublicationHost>())?;
                let publisher = FilePublicationHost::bind_missing(
                    &parent,
                    leaf.to_str().expect("ProjectPath is UTF-8"),
                    recovery.encoded_bytes().len() as u64,
                )
                .map_err(RecoveryStorageError::MissingPublisher)?;
                Ok((publisher, parent))
            })?;
        Ok((reference, pending_bytes, publisher, recovery_parent))
    }

    /// Reads only actual Pending/package bytes and commitments. The concrete operation decodes
    /// its payload outside this owner, then binds that immutable data to these same observations.
    pub(super) fn load_pending(
        root: DirectoryCapability,
        maximum: ByteLimit,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<LoadedPendingRecoveryStorage, RecoveryStorageError> {
        with_candidate_storage(budget, |budget| {
            // The finite owner can return the same raw capture together with one Arc and an
            // admission error. Reserve that largest wrapper before either outcome exists.
            budget.charge_source_bytes(0, 0, 0, size_of::<RecoveryDataBindFailure<()>>())?;
            let (manifest_file, manifest_bytes, manifest_digest, _) = read_file(
                &root,
                PROJECT_MANIFEST_PATH,
                manifest_limit(),
                RecoveryStorageSource::Manifest,
                budget,
            )?;
            let (reference, recovery_file, recovery_digest, recovery_parent, encoded) =
                with_manifest(&manifest_bytes, budget, |mut manifest, budget| {
                    let ProjectAuthoringState::Pending(reference) = std::mem::replace(
                        &mut manifest.authoring.state,
                        ProjectAuthoringState::Ready,
                    ) else {
                        return Err(RecoveryStorageError::PendingRequired);
                    };
                    // This string leaves the temporary parsed-manifest owner with the read token.
                    // Transfer its logical retention before that owner's reservation is released.
                    budget.charge_source_bytes(
                        0,
                        0,
                        0,
                        reference.recovery_path().as_str().len(),
                    )?;
                    let length = usize::try_from(reference.recovery_digest().length())
                        .map_err(|_| RecoveryStorageError::AccountingOverflow)?;
                    if length > maximum.get() {
                        return Err(RecoveryStorageError::PackageEncodedBytesExceeded);
                    }
                    // The actual reference length bounds the native read, independently of the much
                    // larger format ceiling. A hostile digest cannot choose an unadmitted allocation.
                    let read = read_file(
                        &root,
                        reference.recovery_path().as_str(),
                        length,
                        RecoveryStorageSource::Recovery,
                        budget,
                    );
                    let (recovery_file, bytes, recovery_digest, recovery_parent) = match read {
                        Ok(read) => read,
                        Err(RecoveryStorageError::Filesystem { error, .. }) if missing(&error) => {
                            return Err(RecoveryStorageError::PackageMissing);
                        }
                        Err(RecoveryStorageError::Filesystem {
                            error: FsError::ByteLimitExceeded { .. },
                            ..
                        }) => {
                            return Err(RecoveryStorageError::PackageDigestMismatch);
                        }
                        Err(error) => return Err(error),
                    };
                    validate_pending_bytes(
                        &reference,
                        reference.recovery_path().as_str(),
                        Some(&bytes),
                        maximum,
                        budget,
                    )?;
                    verify_named(
                        &root,
                        reference.recovery_path().as_str(),
                        &recovery_file,
                        recovery_digest,
                        RecoveryStorageSource::Recovery,
                        budget,
                    )?;
                    verify_named(
                        &root,
                        PROJECT_MANIFEST_PATH,
                        &manifest_file,
                        manifest_digest,
                        RecoveryStorageSource::Manifest,
                        budget,
                    )?;
                    Ok((
                        reference,
                        recovery_file,
                        recovery_digest,
                        recovery_parent,
                        bytes,
                    ))
                })?;
            Ok(LoadedPendingRecoveryStorage {
                root,
                manifest_file,
                manifest_digest,
                manifest_bytes,
                reference,
                recovery_file,
                recovery_digest,
                recovery_parent,
                encoded,
            })
        })
    }
}

pub(super) struct PreparedRecoveryStorage<P> {
    host: RecoveryStorageHost,
    recovery: Arc<P>,
    reference: ProjectAuthoringRecovery,
    pending_bytes: Vec<u8>,
    publisher: FilePublicationHost,
    recovery_parent: DirectoryCapability,
}

impl<P: RecoveryPackageData> PreparedRecoveryStorage<P> {
    pub(super) fn recovery(&self) -> &P {
        &self.recovery
    }

    #[cfg(test)]
    pub(super) fn test_ready_identity(&self) -> nara_fs::FileIdentity {
        self.host.ready_file.identity()
    }

    #[cfg(test)]
    pub(super) fn test_fail_after_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.publisher.test_fail_after_write()
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        self.publisher.has_cleanup()
    }
    pub(super) fn uncertain(&self) -> bool {
        self.publisher.uncertain()
    }
    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.publisher.retry_cleanup()
    }

    /// Ends an unpublished package attempt only after its actual cleanup and uncertainty have
    /// ended. The enclosing operation settles its retained reservations after these owners drop.
    pub(super) fn release_unpublished(self) -> Result<Arc<P>, Self> {
        if self.publisher.has_cleanup() || self.publisher.uncertain() {
            return Err(self);
        }
        let Self { recovery, .. } = self;
        Ok(recovery)
    }

    pub(super) fn publish(
        mut self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> RecoveryStoragePublication<P> {
        let admitted = verify_named(
            &self.host.root,
            PROJECT_MANIFEST_PATH,
            &self.host.ready_file,
            self.host.ready_digest,
            RecoveryStorageSource::Manifest,
            budget,
        )
        .and_then(|()| {
            with_path_scratch(self.reference.recovery_path().as_str(), budget, |_| {
                let parent =
                    exact_parent(&self.host.root, self.reference.recovery_path().as_str())?;
                if parent.identity() != self.publisher.parent_identity() {
                    return Err(RecoveryStorageError::SourceChanged(
                        RecoveryStorageSource::Recovery,
                    ));
                }
                Ok(())
            })
        })
        .and_then(|()| {
            reserve_publication(
                self.publisher.binding(),
                self.recovery.encoded_bytes().len(),
                self.reference.recovery_path().as_str(),
                budget,
            )
        });
        let reservation = match admitted {
            Ok(reservation) => reservation,
            Err(error) => {
                return RecoveryStoragePublication::Refused {
                    prepared: self,
                    error,
                };
            }
        };
        let outcome = self.publisher.publish(self.recovery.encoded_bytes());
        release(budget, reservation);
        match outcome {
            FilePublicationOutcome::Published(accepted) => {
                let (publication, source) = accepted.into_parts();
                let (capability, digest) = source.into_parts();
                RecoveryStoragePublication::Published(StoredRecovery {
                    host: self.host,
                    recovery: self.recovery,
                    reference: self.reference,
                    pending_bytes: self.pending_bytes,
                    capability,
                    recovery_parent: self.recovery_parent,
                    digest,
                    publication,
                })
            }
            FilePublicationOutcome::Rejected(reason) => RecoveryStoragePublication::Rejected {
                prepared: self,
                reason,
            },
            FilePublicationOutcome::Failed(stage) => RecoveryStoragePublication::Failed {
                prepared: self,
                stage,
            },
            FilePublicationOutcome::Uncertain(publication) => {
                RecoveryStoragePublication::Uncertain {
                    prepared: self,
                    publication,
                }
            }
        }
    }
}

/// Only confirmed package publication constructs this token. It continuously holds the actual
/// package leaf and Ready observation; it grants no source replacement or project adoption.
pub(super) struct StoredRecovery<P> {
    host: RecoveryStorageHost,
    recovery: Arc<P>,
    reference: ProjectAuthoringRecovery,
    pending_bytes: Vec<u8>,
    capability: FileCapability,
    recovery_parent: DirectoryCapability,
    digest: ContentDigest,
    publication: FilePublicationEvidence,
}

impl<P: RecoveryPackageData> StoredRecovery<P> {
    pub(super) fn root(&self) -> &DirectoryCapability {
        &self.host.root
    }
    pub(super) fn recovery(&self) -> &P {
        &self.recovery
    }
    pub(super) fn capability(&self) -> &FileCapability {
        &self.capability
    }
    pub(super) const fn digest(&self) -> ContentDigest {
        self.digest
    }
    pub(super) fn publication(&self) -> &FilePublicationEvidence {
        &self.publication
    }

    /// Ready cancellation leaves the confirmed package on disk and retains its historical
    /// evidence. This releases native custody; it creates no current-file or Pending authority.
    pub(super) fn release_orphan(self) -> (Arc<P>, FilePublicationEvidence) {
        let Self {
            recovery,
            publication,
            ..
        } = self;
        (recovery, publication)
    }

    fn validate(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), RecoveryStorageError> {
        if self.digest.length() != self.reference.recovery_digest().length()
            || self.digest.as_bytes() != self.reference.recovery_digest().as_bytes()
        {
            return Err(RecoveryStorageError::PackageDigestMismatch);
        }
        if self.capability.parent_identity() != Some(self.recovery_parent.identity()) {
            return Err(RecoveryStorageError::SourceChanged(
                RecoveryStorageSource::Recovery,
            ));
        }
        verify_named(
            &self.host.root,
            self.reference.recovery_path().as_str(),
            &self.capability,
            self.digest,
            RecoveryStorageSource::Recovery,
            budget,
        )?;
        verify_named(
            &self.host.root,
            PROJECT_MANIFEST_PATH,
            &self.host.ready_file,
            self.host.ready_digest,
            RecoveryStorageSource::Manifest,
            budget,
        )
    }

    pub(super) fn bind_pending(
        self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<BoundRecoveryPending<P>, RecoveryPendingBindFailure<P>> {
        let result = with_candidate_storage(budget, |budget| {
            self.validate(budget).and_then(|()| {
                with_path_scratch(PROJECT_MANIFEST_PATH, budget, |budget| {
                    // The file owner bounds verification by this observed digest length, keeping
                    // the independent manifest limit as its write policy. Admit the read sentinel too.
                    budget.charge_source_bytes(
                        self.host.ready_bytes.len(),
                        0,
                        add(self.host.ready_bytes.len(), 1)?,
                        size_of::<FilePublicationHost>(),
                    )?;
                    // The additional observed Ready leaf and any retained failed cleanup coexist.
                    budget.retain_open_handles(2)?;
                    budget.admit_handle_peak(HANDLE_PEAK)?;
                    let publisher = FilePublicationHost::bind_existing(
                        &self.host.root,
                        PROJECT_MANIFEST_PATH,
                        &self.host.ready_file,
                        self.host.ready_digest,
                        DEFAULT_MANIFEST_BYTE_LIMIT,
                    )
                    .map_err(RecoveryStorageError::ExistingPublisher)?;
                    if publisher.parent_identity() != self.host.root.identity()
                        || publisher.binding()
                            != (FilePublicationBinding::Existing {
                                identity: self.host.ready_file.identity(),
                                digest: self.host.ready_digest,
                            })
                    {
                        return Err(RecoveryStorageError::SourceChanged(
                            RecoveryStorageSource::Manifest,
                        ));
                    }
                    Ok(publisher)
                })
            })
        });
        match result {
            Ok(publisher) => Ok(BoundRecoveryPending {
                stored: self,
                publisher,
            }),
            Err(error) => Err(RecoveryPendingBindFailure {
                stored: self,
                error,
            }),
        }
    }
}

pub(super) struct BoundRecoveryPending<P> {
    stored: StoredRecovery<P>,
    publisher: FilePublicationHost,
}

impl<P: RecoveryPackageData> BoundRecoveryPending<P> {
    pub(super) const fn recovery_digest(&self) -> ContentDigest {
        self.stored.digest
    }

    #[cfg(test)]
    pub(super) fn test_fail_after_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.publisher.test_fail_after_write()
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        self.publisher.has_cleanup()
    }
    pub(super) fn uncertain(&self) -> bool {
        self.publisher.uncertain()
    }
    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.publisher.retry_cleanup()
    }

    /// Releases only this clean publisher. The concrete operation separately owns its fence
    /// for a possible Pending replacement; no Ready or cancellation permission is inferred.
    pub(super) fn release_before_pending(self) -> Result<StoredRecovery<P>, Self> {
        if self.publisher.has_cleanup() || self.publisher.uncertain() {
            return Err(self);
        }
        let Self { stored, .. } = self;
        Ok(stored)
    }

    /// No ordinary reader is attempted while FilePublicationHost holds its exclusive leaf
    /// lock. Every owner-level read and all output/admission preparation finish before publish.
    pub(super) fn publish(
        mut self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> RecoveryPendingPublication<P> {
        let reservation = match self.stored.validate(budget).and_then(|()| {
            reserve_publication(
                self.publisher.binding(),
                self.stored.pending_bytes.len(),
                PROJECT_MANIFEST_PATH,
                budget,
            )
        }) {
            Ok(reservation) => reservation,
            Err(error) => {
                return RecoveryPendingPublication::Refused {
                    pending: self,
                    error,
                };
            }
        };
        let outcome = self.publisher.publish(&self.stored.pending_bytes);
        release(budget, reservation);
        match outcome {
            FilePublicationOutcome::Published(accepted) => {
                let (pending_publication, source) = accepted.into_parts();
                let (manifest_file, manifest_digest) = source.into_parts();
                // Close both old Ready observations before refunding their handle slots. The
                // remaining publisher slot transfers to the accepted manifest capability.
                drop(self.publisher);
                drop(self.stored.host.ready_file);
                budget.release_open_handles(2);
                RecoveryPendingPublication::Published(PendingRecoveryStorage {
                    observed: ObservedPendingRecoveryStorage {
                        root: self.stored.host.root,
                        manifest_file,
                        manifest_digest,
                        manifest_bytes: self.stored.pending_bytes,
                        reference: self.stored.reference,
                        recovery_file: self.stored.capability,
                        recovery_digest: self.stored.digest,
                        recovery_parent: self.stored.recovery_parent,
                        recovery: self.stored.recovery,
                    },
                    recovery_publication: self.stored.publication,
                    pending_publication,
                })
            }
            FilePublicationOutcome::Rejected(reason) => RecoveryPendingPublication::Rejected {
                pending: self,
                reason,
            },
            FilePublicationOutcome::Failed(stage) => RecoveryPendingPublication::Failed {
                pending: self,
                stage,
            },
            FilePublicationOutcome::Uncertain(publication) => {
                RecoveryPendingPublication::Uncertain {
                    pending: self,
                    publication,
                }
            }
        }
    }
}

/// Actual committed bytes and continuing file observations, before concrete payload admission.
/// There is no decoder callback or operation policy in this transport owner.
pub(super) struct LoadedPendingRecoveryStorage {
    root: DirectoryCapability,
    manifest_file: FileCapability,
    manifest_digest: ContentDigest,
    manifest_bytes: Vec<u8>,
    reference: ProjectAuthoringRecovery,
    recovery_file: FileCapability,
    recovery_digest: ContentDigest,
    recovery_parent: DirectoryCapability,
    encoded: Vec<u8>,
}

pub(super) struct RecoveryDataBindFailure<P> {
    pub(super) loaded: LoadedPendingRecoveryStorage,
    pub(super) recovery: Arc<P>,
    pub(super) error: RecoveryStorageError,
}

impl<P> std::fmt::Debug for RecoveryDataBindFailure<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryDataBindFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl LoadedPendingRecoveryStorage {
    pub(super) fn encoded_bytes(&self) -> &[u8] {
        &self.encoded
    }

    pub(super) fn reference(&self) -> &ProjectAuthoringRecovery {
        &self.reference
    }

    /// Binds the concrete decoder's immutable data to the exact bytes already read. Refusal
    /// retains both owners; matching values do not reconstruct a publication receipt.
    pub(super) fn bind<P: RecoveryPackageData>(
        self,
        recovery: Arc<P>,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<ObservedPendingRecoveryStorage<P>, RecoveryDataBindFailure<P>> {
        let result = with_candidate_storage(budget, |budget| {
            budget.charge_source_bytes(
                0,
                0,
                add(self.encoded.len(), recovery.encoded_bytes().len())?,
                0,
            )?;
            if recovery.digest() != self.reference.recovery_digest()
                || recovery.encoded_bytes() != self.encoded
            {
                return Err(RecoveryStorageError::PackageDigestMismatch);
            }
            validate_storage_path(
                self.reference.recovery_path().as_str(),
                recovery.as_ref(),
                budget,
            )?;
            with_manifest(&self.manifest_bytes, budget, |mut manifest, budget| {
                let ProjectAuthoringState::Pending(reference) =
                    std::mem::replace(&mut manifest.authoring.state, ProjectAuthoringState::Ready)
                else {
                    return Err(RecoveryStorageError::PendingRequired);
                };
                if reference != self.reference {
                    return Err(RecoveryStorageError::ConfigurationMismatch);
                }
                validate_configuration(&manifest, recovery.as_ref(), budget)
            })?;
            verify_named(
                &self.root,
                self.reference.recovery_path().as_str(),
                &self.recovery_file,
                self.recovery_digest,
                RecoveryStorageSource::Recovery,
                budget,
            )?;
            verify_named(
                &self.root,
                PROJECT_MANIFEST_PATH,
                &self.manifest_file,
                self.manifest_digest,
                RecoveryStorageSource::Manifest,
                budget,
            )
        });
        if let Err(error) = result {
            return Err(RecoveryDataBindFailure {
                loaded: self,
                recovery,
                error,
            });
        }
        let Self {
            root,
            manifest_file,
            manifest_digest,
            manifest_bytes,
            reference,
            recovery_file,
            recovery_digest,
            recovery_parent,
            encoded,
        } = self;
        let bytes = encoded.len();
        drop(encoded);
        release(budget, bytes);
        Ok(ObservedPendingRecoveryStorage {
            root,
            manifest_file,
            manifest_digest,
            manifest_bytes,
            reference,
            recovery_file,
            recovery_digest,
            recovery_parent,
            recovery,
        })
    }
}

/// Actual Pending and package observations. A fresh read constructs no publication evidence.
/// Keep these capabilities alive until custody is handed to the enclosing recovery operation.
pub(super) struct ObservedPendingRecoveryStorage<P> {
    root: DirectoryCapability,
    manifest_file: FileCapability,
    manifest_digest: ContentDigest,
    manifest_bytes: Vec<u8>,
    reference: ProjectAuthoringRecovery,
    recovery_file: FileCapability,
    recovery_digest: ContentDigest,
    recovery_parent: DirectoryCapability,
    recovery: Arc<P>,
}

impl<P: RecoveryPackageData> ObservedPendingRecoveryStorage<P> {
    pub(super) fn root(&self) -> &DirectoryCapability {
        &self.root
    }
    pub(super) fn recovery(&self) -> &P {
        &self.recovery
    }
    pub(super) fn shared_recovery(&self) -> Arc<P> {
        Arc::clone(&self.recovery)
    }
    pub(super) fn reference(&self) -> &ProjectAuthoringRecovery {
        &self.reference
    }
    pub(super) fn recovery_file(&self) -> &FileCapability {
        &self.recovery_file
    }
    pub(super) const fn recovery_digest(&self) -> ContentDigest {
        self.recovery_digest
    }
    pub(super) fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }
    pub(super) fn capability(&self) -> &FileCapability {
        &self.manifest_file
    }
    pub(super) const fn digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    pub(super) fn validate(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), RecoveryStorageError> {
        if self.recovery_file.parent_identity() != Some(self.recovery_parent.identity()) {
            return Err(RecoveryStorageError::SourceChanged(
                RecoveryStorageSource::Recovery,
            ));
        }
        verify_named(
            &self.root,
            self.reference.recovery_path().as_str(),
            &self.recovery_file,
            self.recovery_digest,
            RecoveryStorageSource::Recovery,
            budget,
        )?;
        verify_named(
            &self.root,
            PROJECT_MANIFEST_PATH,
            &self.manifest_file,
            self.manifest_digest,
            RecoveryStorageSource::Manifest,
            budget,
        )
    }
}

pub(super) struct PendingRecoveryStorage<P> {
    observed: ObservedPendingRecoveryStorage<P>,
    recovery_publication: FilePublicationEvidence,
    pending_publication: FilePublicationEvidence,
}

impl<P: RecoveryPackageData> PendingRecoveryStorage<P> {
    pub(super) fn observed(&self) -> &ObservedPendingRecoveryStorage<P> {
        &self.observed
    }
    pub(super) fn capability(&self) -> &FileCapability {
        self.observed.capability()
    }
    pub(super) fn digest(&self) -> ContentDigest {
        self.observed.digest()
    }
    pub(super) fn recovery_publication(&self) -> &FilePublicationEvidence {
        &self.recovery_publication
    }
    pub(super) fn pending_publication(&self) -> &FilePublicationEvidence {
        &self.pending_publication
    }
}

#[must_use = "retain the package publisher and actual file outcome for reconciliation"]
pub(super) enum RecoveryStoragePublication<P> {
    Published(StoredRecovery<P>),
    Refused {
        prepared: PreparedRecoveryStorage<P>,
        error: RecoveryStorageError,
    },
    Rejected {
        prepared: PreparedRecoveryStorage<P>,
        reason: FilePublicationRejection,
    },
    Failed {
        prepared: PreparedRecoveryStorage<P>,
        stage: FilePublicationFailureStage,
    },
    Uncertain {
        prepared: PreparedRecoveryStorage<P>,
        publication: FilePublicationEvidence,
    },
}

#[must_use = "retain the Pending publisher, stored package and actual outcome for reconciliation"]
pub(super) enum RecoveryPendingPublication<P> {
    Published(PendingRecoveryStorage<P>),
    Refused {
        pending: BoundRecoveryPending<P>,
        error: RecoveryStorageError,
    },
    Rejected {
        pending: BoundRecoveryPending<P>,
        reason: FilePublicationRejection,
    },
    Failed {
        pending: BoundRecoveryPending<P>,
        stage: FilePublicationFailureStage,
    },
    Uncertain {
        pending: BoundRecoveryPending<P>,
        publication: FilePublicationEvidence,
    },
}

pub(super) struct RecoveryPendingBindFailure<P> {
    pub(super) stored: StoredRecovery<P>,
    pub(super) error: RecoveryStorageError,
}

impl<P> std::fmt::Debug for RecoveryPendingBindFailure<P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryPendingBindFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryStorageSource {
    Manifest,
    Recovery,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RecoveryStorageError {
    #[error("recovery storage exceeded its shared authoring budget")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("the Pending recovery package is missing")]
    PackageMissing,
    #[error("the Pending recovery package exceeds its encoded-byte ceiling")]
    PackageEncodedBytesExceeded,
    #[error("the Pending recovery package does not match its fixed commitment")]
    PackageDigestMismatch,
    #[error("the observed recovery location differs from the actual Pending reference")]
    PackagePathMismatch,
    #[error("the actual {kind:?} filesystem observation failed")]
    Filesystem {
        kind: RecoveryStorageSource,
        #[source]
        error: FsError,
    },
    #[error("the actual {0:?} leaf, parent, name or digest changed")]
    SourceChanged(RecoveryStorageSource),
    #[error("the complete project manifest is invalid")]
    ManifestInvalid,
    #[error("the actual project manifest is not Ready")]
    ReadyRequired,
    #[error("the actual project manifest is not Pending")]
    PendingRequired,
    #[error("the reviewed original manifest differs from the captured Ready bytes")]
    OriginalManifestMismatch,
    #[error("the Pending manifest configuration or source universe differs from recovery data")]
    ConfigurationMismatch,
    #[error("the recovery location is invalid")]
    RecoveryPathInvalid,
    #[error("the recovery location overlaps a participating file, source root or missing sidecar")]
    RecoveryPathConflict,
    #[error("the recovery location already exists")]
    RecoveryAlreadyExists,
    #[error("the package's Missing publisher refused: {0:?}")]
    MissingPublisher(FilePublicationRejection),
    #[error("the exact Ready publisher could not bind: {0:?}")]
    ExistingPublisher(FilePublicationFailureStage),
    #[error("recovery storage accounting overflowed")]
    AccountingOverflow,
}

/// Code-free commitment admission before any concrete payload decoder. Actual-file capture
/// supplies the bytes and retains its own native digest/name evidence separately.
pub(super) fn validate_pending_bytes(
    reference: &ProjectAuthoringRecovery,
    observed_path: &str,
    encoded: Option<&[u8]>,
    maximum: ByteLimit,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), RecoveryStorageError> {
    if observed_path != reference.recovery_path().as_str() {
        return Err(RecoveryStorageError::PackagePathMismatch);
    }
    let encoded = encoded.ok_or(RecoveryStorageError::PackageMissing)?;
    if encoded.len() > maximum.get() {
        return Err(RecoveryStorageError::PackageEncodedBytesExceeded);
    }
    budget.charge_source_bytes(0, 0, encoded.len(), 0)?;
    let digest = ContentDigest::of_bytes(encoded);
    if digest.length() != reference.recovery_digest().length()
        || digest.as_bytes() != reference.recovery_digest().as_bytes()
    {
        return Err(RecoveryStorageError::PackageDigestMismatch);
    }
    Ok(())
}

fn read_file(
    root: &DirectoryCapability,
    path: &str,
    maximum: usize,
    source: RecoveryStorageSource,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(FileCapability, Vec<u8>, ContentDigest, DirectoryCapability), RecoveryStorageError> {
    with_candidate_storage(budget, |budget| {
        with_path_scratch(path, budget, |budget| {
            let (maximum, aggregate_limited) = budget.read_limit(maximum)?;
            // nara_fs retries at most eight bounded snapshots and never overlaps their buffers.
            // Hashing and the ninth sentinel byte/read allowance are also admitted before I/O.
            budget.charge_source_bytes(maximum, 0, add(mul(maximum, 9)?, 8)?, maximum)?;
            let result = (|| {
                let relative = RelativePath::new(path)
                    .map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
                // A non-root parent is a second retained native object. The project root is
                // already supplied by the caller and share() creates no additional handle.
                if path.contains('/') {
                    budget.charge_source_bytes(0, 0, 0, NATIVE_PARENT_BYTES)?;
                }
                budget.retain_open_handles(1 + usize::from(path.contains('/')))?;
                budget.admit_handle_peak(HANDLE_PEAK)?;
                let parent = exact_parent(root, path)?;
                let file = root.open_file_exact(&relative).map_err(|error| {
                    RecoveryStorageError::Filesystem {
                        kind: source,
                        error,
                    }
                })?;
                if file.parent_identity() != Some(parent.identity()) {
                    return Err(RecoveryStorageError::SourceChanged(source));
                }
                let bytes = file.read_to_end_bounded(maximum as u64).map_err(|error| {
                    if aggregate_limited && matches!(error, FsError::ByteLimitExceeded { .. }) {
                        RecoveryStorageError::Admission(
                            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                                maximum: budget.maximum(),
                            },
                        )
                    } else {
                        RecoveryStorageError::Filesystem {
                            kind: source,
                            error,
                        }
                    }
                })?;
                let digest = ContentDigest::of_bytes(&bytes);
                Ok((file, bytes, digest, parent))
            })();
            match result {
                Ok((file, bytes, digest, parent)) => {
                    release(budget, maximum - bytes.len());
                    Ok((file, bytes, digest, parent))
                }
                Err(error) => {
                    release(budget, maximum);
                    Err(error)
                }
            }
        })
    })
}

fn verify_named(
    root: &DirectoryCapability,
    path: &str,
    expected: &FileCapability,
    digest: ContentDigest,
    source: RecoveryStorageSource,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), RecoveryStorageError> {
    with_path_scratch(path, budget, |budget| {
        let bytes = usize::try_from(digest.length())
            .map_err(|_| RecoveryStorageError::AccountingOverflow)?;
        budget.charge_source_bytes(bytes, 0, add(bytes, 1)?, 0)?;
        let relative =
            RelativePath::new(path).map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
        let actual = root
            .open_file_exact(&relative)
            .map_err(|_| RecoveryStorageError::SourceChanged(source))?;
        if actual.identity() != expected.identity()
            || actual.parent_identity() != expected.parent_identity()
            || actual.verify_digest(digest, digest.length()).is_err()
        {
            return Err(RecoveryStorageError::SourceChanged(source));
        }
        Ok(())
    })
}

fn exact_parent(
    root: &DirectoryCapability,
    path: &str,
) -> Result<DirectoryCapability, RecoveryStorageError> {
    match path.rsplit_once('/') {
        Some((parent, _)) => {
            let relative =
                RelativePath::new(parent).map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
            root.open_directory_exact(&relative)
                .map_err(|error| RecoveryStorageError::Filesystem {
                    kind: RecoveryStorageSource::Recovery,
                    error,
                })
        }
        None => Ok(root.share()),
    }
}

pub(super) fn validate_storage_path<P: RecoveryPackageData>(
    path: &str,
    recovery: &P,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), RecoveryStorageError> {
    if path.contains('\\') || path.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return Err(RecoveryStorageError::RecoveryPathInvalid);
    }
    let checked =
        RelativePath::preflight(path).map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
    budget.charge_reference(path.len(), checked.components(), false)?;
    let (count, total, maximum) = recovery.protected_paths().try_fold(
        (0_usize, 0_usize, 0_usize),
        |(count, total, maximum), owned| {
            Ok::<_, RecoveryStorageError>((
                add(count, 1)?,
                add(total, owned.len())?,
                maximum.max(owned.len()),
            ))
        },
    )?;
    let scratch = mul(add(path.len(), maximum)?, 3)?;
    let work = add(mul(total, 3)?, mul(mul(path.len(), 3)?, add(count, 1)?)?)?;
    budget.charge_source_bytes(0, 0, work, 0)?;
    with_temporary(scratch, budget, |_| {
        let path = path.to_lowercase();
        for owned in recovery.protected_paths() {
            let owned = owned.to_lowercase();
            if path == owned || below(&path, &owned) || below(&owned, &path) {
                return Err(RecoveryStorageError::RecoveryPathConflict);
            }
        }
        Ok(())
    })
}

fn below(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn validate_configuration<P: RecoveryPackageData>(
    current: &ProjectManifest,
    recovery: &P,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), RecoveryStorageError> {
    if current.project.source_universe != recovery.source_universe() {
        return Err(RecoveryStorageError::ConfigurationMismatch);
    }
    with_manifest(recovery.old_ready_manifest(), budget, |old, budget| {
        if &old != current {
            return Err(RecoveryStorageError::ConfigurationMismatch);
        }
        with_manifest(recovery.final_ready_manifest(), budget, |future, _| {
            if &future != current {
                return Err(RecoveryStorageError::ConfigurationMismatch);
            }
            Ok(())
        })
    })
}

fn with_manifest<T>(
    bytes: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
    operation: impl FnOnce(
        ProjectManifest,
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<T, RecoveryStorageError>,
) -> Result<T, RecoveryStorageError> {
    // Reuse the bounded nara_project parser. Its actual 4,096-node ceiling bounds overlapping
    // profiles/settings and parser values. This is logical admission, not an RSS guarantee.
    let nodes = bytes.len().min(4_096);
    let slot = add(
        size_of::<nara_project::ProjectProfileOverlay>(),
        size_of::<EffectiveProjectSettings>(),
    )?;
    let reservation = add(
        mul(bytes.len(), 8)?,
        add(mul(nodes, slot)?, size_of::<ProjectManifest>())?,
    )?;
    budget.charge_source_bytes(0, reservation, mul(bytes.len(), 4)?, 0)?;
    with_temporary(reservation, budget, |budget| {
        let parsed = ProjectManifest::parse_toml_bytes(bytes);
        if parsed.has_errors() {
            return Err(RecoveryStorageError::ManifestInvalid);
        }
        let manifest = parsed
            .manifest
            .ok_or(RecoveryStorageError::ManifestInvalid)?;
        operation(manifest, budget)
    })
}

fn encode_manifest(
    manifest: &ProjectManifest,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Vec<u8>, RecoveryStorageError> {
    with_candidate_storage(budget, |budget| {
        let maximum = manifest_limit();
        // Canonical TOML encoding owns its validation and final byte ceiling. Admit overlapping
        // serializer/output storage and escaping before calling it, not after obtaining the Vec.
        let reservation = mul(maximum, 8)?;
        budget.charge_source_bytes(maximum, maximum, mul(maximum, 4)?, reservation)?;
        match manifest
            .to_canonical_toml_bytes()
            .map_err(|_| RecoveryStorageError::ManifestInvalid)
        {
            Ok(bytes) => {
                release(budget, reservation - bytes.len());
                Ok(bytes)
            }
            Err(error) => {
                release(budget, reservation);
                Err(error)
            }
        }
    })
}

fn with_path_scratch<T>(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
    operation: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, RecoveryStorageError>,
) -> Result<T, RecoveryStorageError> {
    // RelativePath stores UTF-16/native components and a component Vec; filesystem opens may
    // temporarily retain two parent handles while traversing each of its validated components.
    budget.charge_source_bytes(0, 0, path.len(), 0)?;
    let preflight =
        RelativePath::preflight(path).map_err(|_| RecoveryStorageError::RecoveryPathInvalid)?;
    budget.charge_reference(path.len(), preflight.components(), true)?;
    budget.charge_open_file(HANDLE_PEAK)?;
    let reservation = add(
        NATIVE_SCRATCH,
        add(mul(path.len(), 4)?, mul(preflight.components(), 128)?)?,
    )?;
    with_temporary(reservation, budget, operation)
}

fn reserve_publication(
    binding: FilePublicationBinding,
    new: usize,
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<usize, RecoveryStorageError> {
    with_candidate_storage(budget, |budget| {
        budget.charge_reference(path.len(), 1, true)?;
        budget.charge_open_file(HANDLE_PEAK)?;
        let cost = binding
            .checked_content_cost(new)
            .ok_or(RecoveryStorageError::AccountingOverflow)?;
        let work = add(cost.content_work_bytes, add(path.len(), 1_024)?)?;
        let reservation = add(NATIVE_SCRATCH, add(new, add(mul(path.len(), 4)?, 4_096)?)?)?;
        budget.charge_source_bytes(cost.encoded_read_bytes, 0, work, reservation)?;
        Ok(reservation)
    })
}

fn with_temporary<T>(
    bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
    operation: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, RecoveryStorageError>,
) -> Result<T, RecoveryStorageError> {
    let retained_before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, 0, bytes) {
        let refused = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .expect("a refused temporary reservation has allocated no storage");
        release(budget, refused);
        return Err(error.into());
    }
    let result = operation(budget);
    release(budget, bytes);
    result
}

/// Inputs are borrowed or returned separately. Only newly produced success values can escape;
/// refusal has dropped every new local before releasing its storage and native handle slots.
fn with_candidate_storage<T>(
    budget: &mut AuthoringProjectAdmissionBudget,
    prepare: impl FnOnce(&mut AuthoringProjectAdmissionBudget) -> Result<T, RecoveryStorageError>,
) -> Result<T, RecoveryStorageError> {
    let retained_before = budget.retained_candidate_bytes;
    let handles_before = budget.retained_open_handles;
    let result = prepare(budget);
    if result.is_err() {
        let released = budget
            .retained_candidate_bytes
            .checked_sub(retained_before)
            .expect("refused storage preparation releases no incumbent storage");
        release(budget, released);
        let closed = budget
            .retained_open_handles
            .checked_sub(handles_before)
            .expect("refused storage preparation releases no incumbent handles");
        budget.release_open_handles(closed);
    }
    result
}

fn release(budget: &mut AuthoringProjectAdmissionBudget, bytes: usize) {
    budget.retained_candidate_bytes = budget
        .retained_candidate_bytes
        .checked_sub(bytes)
        .expect("storage scratch remains charged until its last use returns");
}
fn manifest_limit() -> usize {
    usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT).expect("the manifest limit fits supported targets")
}
fn missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == ErrorKind::NotFound)
}
fn add(left: usize, right: usize) -> Result<usize, RecoveryStorageError> {
    left.checked_add(right)
        .ok_or(RecoveryStorageError::AccountingOverflow)
}
fn mul(left: usize, right: usize) -> Result<usize, RecoveryStorageError> {
    left.checked_mul(right)
        .ok_or(RecoveryStorageError::AccountingOverflow)
}
