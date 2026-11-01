//! Actual persistence-leaf binding for the finite authored Schema upgrade.
//!
//! This does not attest multi-file publication, source meaning or a saved Workspace checkpoint.
//! The enclosing owner validates those and holds every Host guard before final Ready. This
//! guard validates the actual current leaf without replacing the original Host or its policy.

use super::*;

/// The owning validation's read/hash and scratch ceiling, before opening its current leaf.
#[derive(Debug, Clone, Copy)]
pub(in crate::project_host) struct PersistenceAuthoredUpgradeValidationCost {
    pub(in crate::project_host) encoded_bytes: usize,
    pub(in crate::project_host) work_bytes: usize,
    pub(in crate::project_host) scratch_bytes: usize,
    pub(in crate::project_host) transient_handles: usize,
}

impl PersistenceAuthoredUpgradeValidationCost {
    pub(in crate::project_host) fn checked(
        digest: ContentDigest,
        path_bytes: usize,
    ) -> Option<Self> {
        let encoded_bytes = usize::try_from(digest.length()).ok()?.checked_add(1)?;
        Some(Self {
            encoded_bytes,
            work_bytes: encoded_bytes.checked_mul(2)?.checked_add(path_bytes)?,
            scratch_bytes: (2_usize * 65_536)
                .checked_add(path_bytes.checked_mul(8)?)?
                .checked_add(512)?,
            transient_handles: 4,
        })
    }
}

#[must_use]
pub(in crate::project_host) struct PersistenceAuthoredUpgradeCommit<'a> {
    file: &'a mut PersistenceFileState,
    replacement: FileCapability,
    digest: ContentDigest,
}

impl PersistenceAuthoredUpgradeCommit<'_> {
    /// Installs only an already validated file observation. No filesystem work remains here.
    pub(in crate::project_host) fn commit(self) {
        self.file.commit_reopen(self.replacement, self.digest);
    }

    /// Releases only the exclusive destination borrow; the admitted new leaf stays owned.
    pub(in crate::project_host) fn into_replacement(self) -> FileCapability {
        self.replacement
    }
}

#[derive(Debug)]
pub(in crate::project_host) struct PersistenceAuthoredUpgradeBindFailure {
    pub(in crate::project_host) replacement: FileCapability,
    pub(in crate::project_host) reason: EditorPersistenceRejection,
}

impl PersistenceFileState {
    fn bind_authored_upgrade(
        &mut self,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
        replacement: FileCapability,
        digest: ContentDigest,
    ) -> Result<PersistenceAuthoredUpgradeCommit<'_>, PersistenceAuthoredUpgradeBindFailure> {
        let result = self.validate_authored_upgrade(expected, &replacement, digest);
        if let Err(reason) = result {
            return Err(PersistenceAuthoredUpgradeBindFailure {
                replacement,
                reason,
            });
        }
        Ok(PersistenceAuthoredUpgradeCommit {
            file: self,
            replacement,
            digest,
        })
    }

    fn validate_authored_upgrade(
        &self,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
        replacement: &FileCapability,
        digest: ContentDigest,
    ) -> Result<(), EditorPersistenceRejection> {
        if self.publication.has_cleanup() {
            return Err(EditorPersistenceRejection::CleanupPending);
        }
        if self.publication.uncertain() {
            return Err(EditorPersistenceRejection::RequiresReconcile);
        }
        if self.publication.accepted_source_observation() != Some(expected) {
            return Err(EditorPersistenceRejection::TargetChanged);
        }
        if replacement.parent_identity() != Some(self.publication.parent_identity()) {
            return Err(EditorPersistenceRejection::SourceBindingMismatch);
        }
        if digest.length() > file_byte_limit(self.limits) {
            return Err(EditorPersistenceRejection::TargetChanged);
        }
        let current = self
            .publication
            .read_source()
            .open()
            .map_err(|_| EditorPersistenceRejection::TargetChanged)?;
        if current.identity() != replacement.identity()
            || current.parent_identity() != replacement.parent_identity()
        {
            return Err(EditorPersistenceRejection::TargetChanged);
        }
        replacement
            .verify_digest(digest, digest.length())
            .map_err(|_| EditorPersistenceRejection::TargetChanged)?;
        Ok(())
    }
}

impl ScenePersistenceHost {
    pub(in crate::project_host) fn bind_authored_upgrade(
        &mut self,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
        replacement: FileCapability,
        digest: ContentDigest,
    ) -> Result<PersistenceAuthoredUpgradeCommit<'_>, PersistenceAuthoredUpgradeBindFailure> {
        self.file
            .bind_authored_upgrade(expected, replacement, digest)
    }
}

impl PrefabPersistenceHost {
    pub(in crate::project_host) fn bind_authored_upgrade(
        &mut self,
        binding: &nara_tooling::EditorDocumentSourceBinding,
        expected: (FileIdentity, Option<FileIdentity>, ContentDigest),
        replacement: FileCapability,
        digest: ContentDigest,
    ) -> Result<PersistenceAuthoredUpgradeCommit<'_>, PersistenceAuthoredUpgradeBindFailure> {
        if binding != &self.source_binding {
            return Err(PersistenceAuthoredUpgradeBindFailure {
                replacement,
                reason: EditorPersistenceRejection::SourceBindingMismatch,
            });
        }
        self.file
            .bind_authored_upgrade(expected, replacement, digest)
    }
}
