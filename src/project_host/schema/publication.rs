use std::io::Write;

use nara_core::vfs::capabilities::{
    ContentDigest, DirectoryCapability, ExpectedTarget, FileCapability, FileSyncTier, FsError,
    LockMode, RelativeComponent, RelativePath, TemporaryFile, TemporaryFileFailure,
};

use super::{SchemaPublicationFailure as Outcome, SchemaRefreshError};

pub(super) fn publish(
    root: &DirectoryCapability,
    path: &RelativePath,
    original: Option<(FileCapability, ContentDigest)>,
    bytes: &[u8],
    byte_limit: u64,
) -> Result<(), SchemaRefreshError> {
    let (parent, target) = root
        .resolve_file_parent(path)
        .map_err(|_| SchemaRefreshError::Binding)?;
    let expected = original
        .as_ref()
        .map_or(ExpectedTarget::Missing, |(file, _)| {
            ExpectedTarget::Identity(file.identity())
        });
    parent
        .preflight_replace(expected)
        .map_err(|_| publication_error(Outcome::Rejected, false))?;
    let _lock = if let Some((observed, digest)) = original {
        let current = parent
            .open_child_file(&target)
            .map_err(|_| SchemaRefreshError::Binding)?;
        if observed.parent_identity() != Some(parent.identity())
            || current.identity() != observed.identity()
        {
            return Err(SchemaRefreshError::Binding);
        }
        let locked = current.try_lock(LockMode::Exclusive).map_err(|failure| {
            let outcome = match failure.error() {
                FsError::LockContended | FsError::Unsupported { .. } | FsError::Unproven { .. } => {
                    Outcome::Rejected
                }
                _ => Outcome::Failed,
            };
            publication_error(outcome, false)
        })?;
        locked
            .verify_digest(digest, digest.length().min(byte_limit))
            .map_err(|_| SchemaRefreshError::Binding)?;
        Some(locked)
    } else {
        match parent.open_child_file(&target) {
            Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                None
            }
            Ok(_) => return Err(publication_error(Outcome::Rejected, false)),
            Err(_) => return Err(SchemaRefreshError::Binding),
        }
    };

    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > byte_limit {
        return Err(publication_error(Outcome::Failed, false));
    }
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|_| publication_error(Outcome::Failed, false))?;
    let name = RelativeComponent::new(format!(
        ".nara-publication-{:032x}.tmp",
        u128::from_be_bytes(nonce),
    ))
    .expect("the engine-owned temporary filename is valid");
    let mut temporary = parent
        .create_temp(&name)
        .map_err(|failure| cleanup_failure(Outcome::Failed, failure))?;
    if temporary.write_all(bytes).is_err() || temporary.flush().is_err() {
        return Err(discard_temporary(Outcome::Failed, temporary));
    }
    match temporary.sync() {
        Ok(receipt) if receipt.tier() == FileSyncTier::DataAndMetadata => {}
        Ok(_) => return Err(discard_temporary(Outcome::Rejected, temporary)),
        Err(_) => return Err(discard_temporary(Outcome::Failed, temporary)),
    }

    replace_staged_bundle(&parent, &target, expected, temporary)
}

fn replace_staged_bundle(
    parent: &DirectoryCapability,
    target: &RelativeComponent,
    expected: ExpectedTarget,
    temporary: TemporaryFile,
) -> Result<(), SchemaRefreshError> {
    let receipt = parent
        .replace_temp(temporary, target, expected)
        .map_err(|failure| {
            let outcome = match failure.error() {
                FsError::TargetStateMismatch
                | FsError::Unsupported { .. }
                | FsError::Unproven { .. } => Outcome::Rejected,
                _ => Outcome::Failed,
            };
            cleanup_failure(outcome, failure)
        })?;
    let directory_sync_ok = matches!(parent.sync(), Ok(_) | Err(FsError::Unsupported { .. }));
    if !directory_sync_ok
        || receipt.published_identity() != Some(receipt.candidate_identity())
        || receipt.verified_content_digest().is_none()
    {
        return Err(publication_error(Outcome::Uncertain, false));
    }
    Ok(())
}

fn publication_error(outcome: Outcome, cleanup_pending: bool) -> SchemaRefreshError {
    SchemaRefreshError::Publication {
        outcome,
        cleanup_pending,
    }
}

fn cleanup_failure(outcome: Outcome, mut failure: TemporaryFileFailure) -> SchemaRefreshError {
    publication_error(
        outcome,
        !failure.retry_cleanup() && !failure.retry_cleanup(),
    )
}

fn discard_temporary(outcome: Outcome, temporary: TemporaryFile) -> SchemaRefreshError {
    publication_error(
        outcome,
        temporary
            .discard()
            .is_err_and(|mut failure| !failure.retry_cleanup()),
    )
}

#[cfg(all(test, target_arch = "x86_64", any(windows, target_os = "linux")))]
mod tests {
    use std::fs;

    use super::*;
    use crate::project_host::schema::tests::TestProject;
    use crate::test_host_fs as host_fs;

    const BUNDLE: &str = "schema/authoring.bundle.json";
    const BYTE_LIMIT: u64 = 1024;

    #[test]
    fn changed_bytes_are_rejected_before_staging() {
        let prior = b"prior";
        for changed in [b"other".as_slice(), b"longer than prior"] {
            let project = TestProject::new();
            fs::write(project.bundle(), prior).unwrap();
            let root = host_fs::writable_directory(&project.0).unwrap();
            let path = RelativePath::new(BUNDLE).unwrap();
            let observed = root.open_file(&path).unwrap();
            let identity = observed.identity();
            fs::write(project.bundle(), changed).unwrap();

            assert_eq!(
                publish(
                    &root,
                    &path,
                    Some((observed, ContentDigest::of_bytes(prior))),
                    b"successor",
                    BYTE_LIMIT
                ),
                Err(SchemaRefreshError::Binding),
            );
            assert_eq!(root.open_file(&path).unwrap().identity(), identity);
            assert_eq!(fs::read(project.bundle()).unwrap(), changed);
            assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
        }
    }

    #[test]
    fn replacement_leaf_with_identical_bytes_is_rejected_before_staging() {
        let project = TestProject::new();
        let prior = b"prior";
        fs::write(project.bundle(), prior).unwrap();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let path = RelativePath::new(BUNDLE).unwrap();
        let observed = root.open_file(&path).unwrap();
        let moved = project.0.join("schema/prior.json");
        fs::rename(project.bundle(), &moved).unwrap();
        fs::write(project.bundle(), prior).unwrap();
        let replacement = root.open_file(&path).unwrap();
        assert_ne!(observed.identity(), replacement.identity());

        assert_eq!(
            publish(
                &root,
                &path,
                Some((observed, ContentDigest::of_bytes(prior))),
                b"successor",
                BYTE_LIMIT
            ),
            Err(SchemaRefreshError::Binding),
        );
        assert_eq!(
            root.open_file(&path).unwrap().identity(),
            replacement.identity()
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), prior);
        assert_eq!(fs::read(moved).unwrap(), prior);
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 2);
    }

    #[test]
    fn a_cooperating_writer_blocks_replacement() {
        let project = TestProject::new();
        let prior = b"prior";
        fs::write(project.bundle(), prior).unwrap();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let path = RelativePath::new(BUNDLE).unwrap();
        let observed = root.open_file(&path).unwrap();
        let guard = root
            .open_file(&path)
            .unwrap()
            .try_lock(LockMode::Exclusive)
            .unwrap();

        assert_eq!(
            publish(
                &root,
                &path,
                Some((observed, ContentDigest::of_bytes(prior))),
                b"successor",
                BYTE_LIMIT
            ),
            Err(publication_error(Outcome::Rejected, false)),
        );
        drop(guard);
        assert_eq!(fs::read(project.bundle()).unwrap(), prior);
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }

    #[test]
    fn file_created_since_a_missing_observation_is_preserved() {
        let project = TestProject::new();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let path = RelativePath::new(BUNDLE).unwrap();
        fs::write(project.bundle(), b"other writer").unwrap();
        let identity = root.open_file(&path).unwrap().identity();

        assert_eq!(
            publish(&root, &path, None, b"successor", BYTE_LIMIT),
            Err(publication_error(Outcome::Rejected, false)),
        );
        assert_eq!(root.open_file(&path).unwrap().identity(), identity);
        assert_eq!(fs::read(project.bundle()).unwrap(), b"other writer");
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }

    fn synced_candidate(parent: &DirectoryCapability) -> TemporaryFile {
        let mut temporary = parent
            .create_temp(&RelativeComponent::new("candidate.tmp").unwrap())
            .unwrap();
        temporary.write_all(b"successor").unwrap();
        assert_eq!(
            temporary.sync().unwrap().tier(),
            FileSyncTier::DataAndMetadata
        );
        temporary
    }

    #[test]
    fn name_switch_with_unverified_bytes_is_uncertain() {
        let project = TestProject::new();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let (parent, target) = root
            .resolve_file_parent(&RelativePath::new(BUNDLE).unwrap())
            .unwrap();
        let temporary = synced_candidate(&parent);
        fs::write(project.0.join("schema/candidate.tmp"), b"different").unwrap();

        assert_eq!(
            replace_staged_bundle(&parent, &target, ExpectedTarget::Missing, temporary),
            Err(publication_error(Outcome::Uncertain, false)),
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), b"different");
        assert_eq!(fs::read_dir(project.0.join("schema")).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn failed_schema_write_preserves_prior_bytes_and_reports_pending_cleanup() {
        use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

        let project = TestProject::new();
        let prior = b"prior bundle";
        fs::write(project.bundle(), prior).unwrap();
        let root = host_fs::writable_directory(&project.0).unwrap();
        let path = RelativePath::new(BUNDLE).unwrap();
        let (parent, target) = root.resolve_file_parent(&path).unwrap();
        let observed = parent.open_child_file(&target).unwrap();
        let blocker = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(project.bundle())
            .unwrap();
        let temporary = synced_candidate(&parent);
        let candidate = project.0.join("schema/candidate.tmp");
        let permissions = fs::metadata(&candidate).unwrap().permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&candidate, readonly).unwrap();

        let error = replace_staged_bundle(
            &parent,
            &target,
            ExpectedTarget::Identity(observed.identity()),
            temporary,
        )
        .unwrap_err();
        fs::set_permissions(&candidate, permissions).unwrap();
        drop(blocker);
        assert_eq!(error, publication_error(Outcome::Failed, true));
        assert_eq!(
            root.open_file(&path).unwrap().identity(),
            observed.identity()
        );
        assert_eq!(fs::read(project.bundle()).unwrap(), prior);
        assert_eq!(fs::read(&candidate).unwrap(), b"successor");
        fs::remove_file(candidate).unwrap();

        let failure = crate::project_tooling::test_schema_failure_protocol_roundtrip(error);
        assert_eq!(failure.operation(), "schema-write");
        assert_eq!(
            failure.stage(),
            crate::project_tooling::ProjectToolFailureStage::Schema
        );
        assert_eq!(
            failure.code(),
            "project.tool.schema-publication-failed-cleanup-pending"
        );
    }
}
