use std::fs;

use nara_fs::{DirectorySyncTier, HostCapabilityOptions, StageStatus, TrustMode};

use super::super::tests::Release;
use super::*;

const INSTALLATION_PATH: &str = ".nara/source-tools/artifacts/installed";

fn installation_path(release: &Release) -> std::path::PathBuf {
    release.directory.join(INSTALLATION_PATH)
}

fn project_root(release: &Release) -> DirectoryCapability {
    use std::os::windows::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(&release.directory)
        .unwrap();
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn plan(release: &Release) -> EditorSourceToolInstallationPlan {
    let digest = release.write_manifest();
    let candidate =
        PreparedEditorSourceToolRelease::capture(&release.root(TrustMode::Untrusted), digest)
            .unwrap();
    EditorSourceToolInstallationPlan::new(
        project_root(release),
        RelativeComponent::new("installed").unwrap(),
        candidate,
    )
    .unwrap()
}

fn published(outcome: EditorSourceToolInstallationOutcome) -> EditorSourceToolInstallation {
    match outcome {
        EditorSourceToolInstallationOutcome::Published(value) => value,
        other => panic!("expected real no-replace installation publication: {other:?}"),
    }
}

#[test]
fn real_windows_installation_preserves_created_objects_and_reopens_receipt_selected_bytes() {
    let source = Release::new();
    let plan = plan(&source);
    let expected_manifest = plan.release().manifest_digest();
    assert_eq!(plan.project_path(), INSTALLATION_PATH);
    assert!(plan.same_project(&project_root(&source)).unwrap());
    assert!(!source.directory.join(".nara").exists());
    let installation = published(plan.publish());
    assert_eq!(installation.project_path(), INSTALLATION_PATH);
    assert!(installation.same_project(&project_root(&source)).unwrap());
    assert!(
        installation
            .project_root()
            .same_object(&project_root(&source))
            .unwrap()
    );
    assert_eq!(installation.package(), "test.stats");
    assert_eq!(installation.receipt().manifest_digest(), expected_manifest);
    assert_eq!(installation.members().len(), 5);
    assert!(
        installation
            .members()
            .iter()
            .any(|file| file.path() == "samples/readme.txt"
                && file.kind() == Some(EditorSourceToolReleaseMemberKind::Content))
    );
    assert!(
        installation
            .members()
            .iter()
            .all(|file| file.sync_receipt().progress().data_synced() == StageStatus::Achieved)
    );
    assert_eq!(
        installation
            .publication()
            .durability()
            .parent_directory_synced(),
        StageStatus::Unsupported
    );
    assert_eq!(
        installation.directory().options().rights(),
        CapabilityRights::ReadWrite
    );
    assert_eq!(
        nara_fs::platform_capability_matrix().directory_sync_tier(),
        DirectorySyncTier::Unsupported
    );
    installation.verify().unwrap();
    let expected_receipt = installation.receipt_digest();
    assert_eq!(
        installation.receipt_publication().verified_content_digest(),
        Some(expected_receipt)
    );
    let installed_id = *installation.receipt().installation_id();
    drop(installation);
    assert!(installation_path(&source).join("Tool.dll").is_file());
    assert!(!source.directory.join("installed").exists());
    let reopened = project_root(&source)
        .open_directory_exact(&RelativePath::new(INSTALLATION_PATH).unwrap())
        .unwrap();
    let inspected =
        EditorSourceToolInstallationInspection::read(&reopened, expected_receipt).unwrap();
    assert_eq!(inspected.receipt().installation_id(), &installed_id);
    assert_eq!(inspected.receipt_digest(), expected_receipt);
    assert_eq!(inspected.directory_identity(), reopened.identity());
    assert_eq!(inspected.release().manifest_digest(), expected_manifest);
    assert_eq!(
        inspected
            .release()
            .members()
            .iter()
            .find(|file| file.path() == "samples/readme.txt")
            .unwrap()
            .bytes(),
        b"editable sample"
    );
}

#[test]
fn real_windows_successor_installation_reinspects_the_exact_captured_catalog_pair() {
    let source = Release::creation_metadata_successor();
    let plan = plan(&source);
    let expected_manifest = plan.release().manifest_digest();
    let expected_owner = plan.release().schema_owner().clone();
    let expected_catalogs = ["catalog.json", "catalog-predecessor.json"].map(|path| {
        let member = plan
            .release()
            .members()
            .iter()
            .find(|member| member.path() == path)
            .unwrap();
        (path, member.bytes().to_vec())
    });
    let installation = published(plan.publish());
    assert_eq!(
        installation.catalog_predecessor(),
        Some("catalog-predecessor.json")
    );
    assert_eq!(installation.members().len(), 6);
    assert!(installation.members().iter().any(|member| {
        member.path() == "catalog-predecessor.json"
            && member.kind() == Some(EditorSourceToolReleaseMemberKind::Tool)
    }));
    installation.verify().unwrap();
    let expected_receipt = installation.receipt_digest();
    drop(installation);

    let reopened = project_root(&source)
        .open_directory_exact(&RelativePath::new(INSTALLATION_PATH).unwrap())
        .unwrap();
    let inspected =
        EditorSourceToolInstallationInspection::read(&reopened, expected_receipt).unwrap();
    let release = inspected.release();
    assert_eq!(release.manifest_digest(), expected_manifest);
    assert_eq!(
        release.catalog_predecessor(),
        Some("catalog-predecessor.json")
    );
    assert_eq!(release.schema_owner().current(), expected_owner.current());
    assert_eq!(
        release.schema_owner().predecessor(),
        expected_owner.predecessor()
    );
    for (path, bytes) in expected_catalogs {
        let member = release
            .members()
            .iter()
            .find(|member| member.path() == path)
            .unwrap();
        assert_eq!(member.bytes(), bytes);
        assert_eq!(
            fs::read(installation_path(&source).join(path)).unwrap(),
            bytes
        );
    }
    drop(inspected);

    let predecessor_path = installation_path(&source).join("catalog-predecessor.json");
    let mut changed_bytes = fs::read(&predecessor_path).unwrap();
    changed_bytes[0] ^= 1;
    fs::write(predecessor_path, changed_bytes).unwrap();
    assert!(matches!(
        EditorSourceToolInstallationInspection::read(&reopened, expected_receipt),
        Err(EditorSourceToolInstallationError::Inspection(
            super::super::EditorSourceToolReleaseError::Digest { member }
        )) if member == "catalog-predecessor.json"
    ));
}

#[test]
fn inspection_retains_the_live_read_only_root_without_execution_or_deletion() {
    use std::os::windows::fs::OpenOptionsExt;

    let source = Release::new();
    let installation = published(plan(&source).publish());
    let expected_receipt = installation.receipt_digest();
    drop(installation);
    let installed = installation_path(&source);
    let open_root = |sharing| {
        fs::OpenOptions::new()
            .read(true)
            .share_mode(sharing)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(&installed)
    };
    let root = DirectoryCapability::from_host_handle(
        open_root(7).unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
    )
    .unwrap();
    let identity = root.identity();
    let inspected = EditorSourceToolInstallationInspection::read(&root, expected_receipt).unwrap();
    drop(root);
    assert_eq!(inspected.directory_identity(), identity);
    assert_eq!(
        inspected.directory().options().rights(),
        CapabilityRights::ReadOnly
    );
    // Only the inspection now owns the directory handle; an exclusive read must still conflict.
    let exclusive = open_root(0).unwrap_err();
    assert_eq!(
        exclusive.raw_os_error(),
        Some(windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION as i32),
        "the live inspected read handle must prevent an exclusive open: {exclusive}",
    );
    assert_eq!(
        inspected
            .directory()
            .open_file_exact(&RelativePath::new("Tool.dll").unwrap())
            .unwrap()
            .read_to_end_bounded(128)
            .unwrap(),
        b"not an executable; capture must only read"
    );
    drop(inspected);
    assert!(
        open_root(0).is_ok(),
        "inspection retirement must close its root"
    );
    assert_eq!(
        fs::read(installed.join("Tool.dll")).unwrap(),
        b"not an executable; capture must only read"
    );
}

#[test]
fn inspection_binds_exact_receipt_bytes_instead_of_reencoded_facts() {
    let source = Release::new();
    let installation = published(plan(&source).publish());
    let original_digest = installation.receipt_digest();
    let original_receipt = installation.receipt().clone();
    drop(installation);
    let receipt_path = installation_path(&source).join(RECEIPT_NAME);
    let mut bytes = fs::read(&receipt_path).unwrap();
    bytes.push(b'\n');
    let expected_receipt = ContentDigest::of_bytes(&bytes);
    assert_ne!(expected_receipt, original_digest);
    fs::write(&receipt_path, &bytes).unwrap();
    let root = project_root(&source)
        .open_directory_exact(&RelativePath::new(INSTALLATION_PATH).unwrap())
        .unwrap();
    assert!(EditorSourceToolInstallationInspection::read(&root, original_digest).is_err());
    let inspected = EditorSourceToolInstallationInspection::read(&root, expected_receipt).unwrap();
    assert_eq!(inspected.receipt(), &original_receipt);
    assert_eq!(inspected.receipt_digest(), expected_receipt);
    assert_eq!(fs::read(&receipt_path).unwrap(), bytes);
}

#[test]
fn cancelling_the_preview_keeps_the_exact_candidate_and_creates_no_installation() {
    let source = Release::new();
    let plan = plan(&source);
    let digest = plan.release().manifest_digest();
    let candidate = plan.cancel();
    assert_eq!(candidate.manifest_digest(), digest);
    assert!(!source.directory.join(".nara").exists());
    assert!(!fs::read_dir(&source.directory).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nara-tool-stage-")
    }));
}

#[test]
fn dropping_the_preview_creates_no_store_and_keeps_project_objects_untouched() {
    let source = Release::new();
    fs::write(source.directory.join("keep.txt"), b"project source").unwrap();
    let preview = plan(&source);
    assert_eq!(preview.project_path(), INSTALLATION_PATH);
    drop(preview);
    assert!(!source.directory.join(".nara").exists());
    assert_eq!(
        fs::read(source.directory.join("keep.txt")).unwrap(),
        b"project source"
    );
}

#[test]
fn project_association_compares_actual_roots_across_host_sessions() {
    let source = Release::new();
    let other = Release::new();
    let preview = plan(&source);
    let same_root = project_root(&source);
    let other_root = project_root(&other);
    assert_ne!(preview.project_root().identity(), same_root.identity());
    assert!(preview.same_project(&same_root).unwrap());
    assert!(!preview.same_project(&other_root).unwrap());
    let installation = published(preview.publish());
    assert!(installation.same_project(&same_root).unwrap());
    assert!(!installation.same_project(&other_root).unwrap());
    installation.verify().unwrap();
    assert!(!other.directory.join(".nara").exists());
}

#[test]
fn invalid_destination_or_read_only_project_rejects_without_creating_store_parents() {
    let source = Release::new();
    for name in ["工具", "café"] {
        let candidate = plan(&source).cancel();
        assert!(matches!(
            EditorSourceToolInstallationPlan::new(
                project_root(&source),
                RelativeComponent::new(name).unwrap(),
                candidate,
            ),
            Err(EditorSourceToolInstallationError::InvalidDestination)
        ));
    }
    let candidate = plan(&source).cancel();
    assert!(matches!(
        EditorSourceToolInstallationPlan::new(
            source.root(TrustMode::Untrusted),
            RelativeComponent::new("installed").unwrap(),
            candidate,
        ),
        Err(EditorSourceToolInstallationError::ReadOnly)
    ));
    assert!(!source.directory.join(".nara").exists());
}

#[test]
fn publication_refuses_trusted_local_traversal_before_any_store_write() {
    use std::os::windows::fs::OpenOptionsExt;

    let source = Release::new();
    let candidate = plan(&source).cancel();
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(&source.directory)
        .unwrap();
    let project = DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap();
    assert!(matches!(
        EditorSourceToolInstallationPlan::new(
            project,
            RelativeComponent::new("installed").unwrap(),
            candidate,
        ),
        Err(EditorSourceToolInstallationError::StrictCapabilityRequired)
    ));
    assert!(!source.directory.join(".nara").exists());
}

#[test]
fn publish_refuses_a_file_at_each_fixed_store_parent_without_overwrite_or_cleanup() {
    for blocker in [".nara", ".nara/source-tools", artifact_store::PATH] {
        let source = Release::new();
        let preview = plan(&source);
        let path = source.directory.join(blocker);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"unowned parent blocker").unwrap();
        assert!(matches!(
            preview.publish(),
            EditorSourceToolInstallationOutcome::Failed {
                error: EditorSourceToolInstallationError::ArtifactStore(_),
                orphan: None,
                ..
            }
        ));
        assert_eq!(fs::read(&path).unwrap(), b"unowned parent blocker");
        assert!(!installation_path(&source).exists());
    }
}

#[test]
fn publish_reuses_fixed_parents_and_preserves_unrelated_metadata_and_artifact_siblings() {
    let source = Release::new();
    fs::create_dir_all(source.directory.join(artifact_store::PATH)).unwrap();
    let selection = source.directory.join(".nara/source-tools/selection.json");
    let sibling = source
        .directory
        .join(artifact_store::PATH)
        .join("unowned.txt");
    fs::write(&selection, b"durable project metadata").unwrap();
    fs::write(&sibling, b"unknown sibling").unwrap();
    let installation = published(plan(&source).publish());
    installation.verify().unwrap();
    assert_eq!(fs::read(&selection).unwrap(), b"durable project metadata");
    assert_eq!(fs::read(&sibling).unwrap(), b"unknown sibling");
}

#[test]
fn conflicting_destination_is_never_overwritten_or_claimed() {
    let source = Release::new();
    let plan = plan(&source);
    fs::create_dir_all(installation_path(&source)).unwrap();
    fs::write(installation_path(&source).join("keep.txt"), "unowned").unwrap();
    let EditorSourceToolInstallationOutcome::Failed {
        error:
            EditorSourceToolInstallationError::Begin {
                source: error,
                parent,
            },
        orphan: None,
        ..
    } = plan.publish()
    else {
        panic!("an existing installation destination must refuse before staging");
    };
    assert!(matches!(
        error.before_staging_error(),
        Some(FsError::TargetStateMismatch)
    ));
    assert!(
        parent
            .same_object(
                &project_root(&source)
                    .open_directory_exact(&RelativePath::new(artifact_store::PATH).unwrap())
                    .unwrap()
            )
            .unwrap()
    );
    assert_eq!(
        fs::read(installation_path(&source).join("keep.txt")).unwrap(),
        b"unowned"
    );
    assert!(!installation_path(&source).join("nara-tool.json").exists());
}

#[test]
fn payload_failure_retains_the_unready_published_directory_and_actual_cleanup_custody() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| {
            Err(EditorSourceToolInstallationError::Receipt(
                "injected before publish",
            ))
        },
        |_, _| Ok(()),
    );
    let EditorSourceToolInstallationOutcome::Incomplete {
        error, publication, ..
    } = outcome
    else {
        panic!("unready published candidate must retain actual custody");
    };
    assert!(matches!(
        error,
        EditorSourceToolInstallationError::Receipt("injected before publish")
    ));
    assert_eq!(
        publication.publication().published_identity(),
        publication.directory().identity()
    );
    assert_eq!(publication.pending_files(), 4);
    assert!(installation_path(&source).exists());
    assert!(!installation_path(&source).join(RECEIPT_NAME).exists());
    assert!(
        publication
            .store_parent()
            .same_object(
                &project_root(&source)
                    .open_directory_exact(&RelativePath::new(artifact_store::PATH).unwrap())
                    .unwrap()
            )
            .unwrap()
    );
    assert_eq!(publication.members().len(), 4);
    assert!(
        publication
            .directory()
            .open_directory(&RelativePath::new("samples").unwrap())
            .is_ok()
    );
    drop(publication);
    assert!(installation_path(&source).join("Tool.dll").is_file());
}

#[test]
fn incomplete_cleanup_handover_preserves_replaced_original_until_explicit_repair() {
    let source = Release::new();
    let outcome = plan(&source).publish_failing_for_tests(PublicationTestFault::MemberCreated(0));
    let EditorSourceToolInstallationOutcome::Incomplete {
        mut publication, ..
    } = outcome
    else {
        panic!("actual created member must remain incomplete");
    };
    assert_eq!(publication.members().len(), 1);
    assert_eq!(publication.pending_files(), 1);
    assert_eq!(publication.members()[0].path(), MANIFEST_NAME);
    assert_eq!(publication.members()[0].sync_receipt(), None);
    let directory = installation_path(&source);
    let original = directory.join(MANIFEST_NAME);
    let renamed = directory.join("original-manifest");
    let bytes = fs::read(&original).unwrap();
    fs::rename(&original, &renamed).unwrap();
    fs::write(&original, &bytes).unwrap();
    assert!(publication.hand_over_readers().is_err());
    assert_eq!(publication.pending_files(), 1);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(fs::read(&renamed).unwrap(), bytes);
    fs::remove_file(&original).unwrap();
    fs::rename(&renamed, &original).unwrap();
    publication.hand_over_readers().unwrap();
    assert_eq!(publication.pending_files(), 0);
    assert!(publication.members()[0].file().is_some());
    let lease = publication.directory().acquire_namespace_lease().unwrap();
    assert_eq!(fs::read(&original).unwrap(), bytes);
    drop(lease);
    drop(publication);
    assert!(original.is_file());
}

#[test]
fn failed_receipt_commit_retains_the_actual_temporary_path_and_competing_receipt() {
    let source = Release::new();
    let outcome = plan(&source).publish_failing_for_tests(PublicationTestFault::CompetingReceipt);
    let EditorSourceToolInstallationOutcome::Incomplete {
        mut publication, ..
    } = outcome
    else {
        panic!("the actual competing receipt must refuse no-replace publication");
    };
    assert_eq!(publication.members().len(), 5);
    assert!(
        !publication
            .members()
            .iter()
            .any(|member| member.path() == RECEIPT_NAME)
    );
    let temporary = publication
        .members()
        .iter()
        .find(|member| member.role() == EditorSourceToolArtifactRole::ReceiptTemporary)
        .unwrap();
    assert!(temporary.path().starts_with(".nara-tool-stage-"));
    assert!(temporary.sync_receipt().is_some());
    let temporary_path = installation_path(&source).join(temporary.path());
    let bytes = fs::read(&temporary_path).unwrap();
    assert_eq!(ContentDigest::of_bytes(&bytes), temporary.baseline());
    publication.hand_over_readers().unwrap();
    assert_eq!(publication.pending_files(), 0);
    assert_eq!(
        fs::read(installation_path(&source).join(RECEIPT_NAME)).unwrap(),
        b"independent competing receipt"
    );
    drop(publication);
    assert_eq!(fs::read(temporary_path).unwrap(), bytes);
}

#[test]
fn post_publication_member_replacement_preserves_published_evidence_and_original_custody() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| Ok(()),
        |_, _| {
            fs::rename(
                installation_path(&source).join("Tool.dll"),
                installation_path(&source).join("original.dll"),
            )
            .unwrap();
            fs::write(
                installation_path(&source).join("Tool.dll"),
                b"not an executable; capture must only read",
            )
            .unwrap();
            Ok(())
        },
    );
    let EditorSourceToolInstallationOutcome::PublishedUnverified {
        installation,
        error,
    } = outcome
    else {
        panic!("same bytes in another object must reject published admission");
    };
    assert!(
        matches!(error, EditorSourceToolInstallationError::Changed { member } if member == "Tool.dll")
    );
    assert_eq!(
        installation.publication().published_identity(),
        installation.directory().identity()
    );
    assert!(installation_path(&source).join("original.dll").is_file());
    assert!(installation.verify().is_err());
}

#[test]
fn retained_members_prevent_directory_rename_on_the_qualified_windows_filesystem() {
    let source = Release::new();
    let installation = published(plan(&source).publish());
    let error = fs::rename(
        installation_path(&source),
        source.directory.join("moved-installation"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(installation_path(&source).join("Tool.dll").is_file());
    assert!(!source.directory.join("moved-installation").exists());
    installation.verify().unwrap();
}

#[test]
fn undeclared_payload_prevents_ready_commit_and_survives_candidate_retirement() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| {
            fs::write(installation_path(&source).join("unknown.dll"), b"unowned").unwrap();
            Ok(())
        },
        |_, _| Ok(()),
    );
    let EditorSourceToolInstallationOutcome::Incomplete { publication, .. } = outcome else {
        panic!("undeclared payload must prevent ready publication");
    };
    assert!(!installation_path(&source).join(RECEIPT_NAME).exists());
    assert_eq!(publication.pending_files(), 4);
    fs::write(
        installation_path(&source).join("samples/readme.txt"),
        b"user edit after failure",
    )
    .unwrap();
    drop(publication);
    assert_eq!(
        fs::read(installation_path(&source).join("unknown.dll")).unwrap(),
        b"unowned"
    );
    assert_eq!(
        fs::read(installation_path(&source).join("samples/readme.txt")).unwrap(),
        b"user edit after failure"
    );
}

#[test]
fn post_ready_undeclared_member_requires_reconciliation_without_losing_publication() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| Ok(()),
        |_, _| {
            fs::write(installation_path(&source).join("unknown.dll"), b"unowned").unwrap();
            Ok(())
        },
    );
    let EditorSourceToolInstallationOutcome::PublishedUnverified { installation, .. } = outcome
    else {
        panic!("unexpected installed files must not silently join the release closure");
    };
    assert_eq!(
        installation.receipt_publication().verified_content_digest(),
        Some(installation.receipt_digest())
    );
    assert!(installation.verify().is_err());
    let root = installation_path(&source);
    let expected = installation.receipt_digest();
    assert!(
        EditorSourceToolInstallationInspection::read(installation.directory(), expected).is_err()
    );
    drop(installation);
    assert_eq!(fs::read(root.join("unknown.dll")).unwrap(), b"unowned");
}

#[test]
fn a_competing_receipt_is_not_overwritten_or_claimed() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| {
            fs::write(
                installation_path(&source).join(RECEIPT_NAME),
                b"unowned receipt",
            )
            .unwrap();
            Ok(())
        },
        |_, _| Ok(()),
    );
    let EditorSourceToolInstallationOutcome::Incomplete { publication, .. } = outcome else {
        panic!("receipt collision must refuse the ready commit");
    };
    assert!(publication.pending_files() > 0);
    drop(publication);
    assert_eq!(
        fs::read(installation_path(&source).join(RECEIPT_NAME)).unwrap(),
        b"unowned receipt"
    );
}

#[test]
fn receipt_names_are_reserved_before_any_destination_mutation() {
    let mut source = Release::new();
    fs::rename(
        source.directory.join("samples/readme.txt"),
        source.directory.join(".NARA-INSTALLATION.JSON"),
    )
    .unwrap();
    source.manifest["files"][2]["path"] = serde_json::json!(".NARA-INSTALLATION.JSON");
    let digest = source.write_manifest();
    let candidate =
        PreparedEditorSourceToolRelease::capture(&source.root(TrustMode::Untrusted), digest)
            .unwrap();
    assert!(matches!(
        EditorSourceToolInstallationPlan::new(
            project_root(&source),
            RelativeComponent::new("installed").unwrap(),
            candidate
        ),
        Err(EditorSourceToolInstallationError::ReservedPath)
    ));
    assert!(!source.directory.join(".nara").exists());
}

#[test]
fn inspection_checks_strict_source_authority_before_reading_any_receipt() {
    let source = Release::new();
    assert!(matches!(
        EditorSourceToolInstallationInspection::read(
            &source.root(TrustMode::TrustedLocal),
            ContentDigest::of_bytes(b"not read")
        ),
        Err(EditorSourceToolInstallationError::Inspection(
            super::super::super::EditorSourceToolReleaseError::SourceTrust
        ))
    ));
}

#[test]
fn changing_the_ready_receipt_retains_both_publication_facts_and_requires_reconciliation() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| Ok(()),
        |_, _| {
            let path = installation_path(&source).join(RECEIPT_NAME);
            let mut bytes = fs::read(&path).unwrap();
            bytes[0] ^= 1;
            fs::write(path, bytes).unwrap();
            Ok(())
        },
    );
    let EditorSourceToolInstallationOutcome::PublishedUnverified {
        installation,
        error,
    } = outcome
    else {
        panic!("a committed receipt cannot become an unpublished failure");
    };
    assert!(
        matches!(error, EditorSourceToolInstallationError::Changed { member } if member == RECEIPT_NAME)
    );
    assert_eq!(
        installation.publication().published_identity(),
        installation.directory().identity()
    );
    assert_eq!(
        installation.receipt_publication().verified_content_digest(),
        Some(installation.receipt_digest())
    );
    assert!(installation.verify().is_err());
}
