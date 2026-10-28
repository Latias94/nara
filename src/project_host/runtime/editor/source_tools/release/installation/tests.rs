use std::fs;

use nara_fs::{DirectorySyncTier, HostCapabilityOptions, StageStatus, TrustMode};

use super::super::tests::Release;
use super::*;

fn parent(release: &Release) -> DirectoryCapability {
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
        parent(release),
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
    let installation = published(plan.publish());
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
    assert!(source.directory.join("installed/Tool.dll").is_file());
    let reopened = parent(&source)
        .open_directory_exact(&RelativePath::new("installed").unwrap())
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
fn inspection_retains_the_live_read_only_root_without_execution_or_deletion() {
    use std::os::windows::fs::OpenOptionsExt;

    let source = Release::new();
    let installation = published(plan(&source).publish());
    let expected_receipt = installation.receipt_digest();
    drop(installation);
    let installed = source.directory.join("installed");
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
    let receipt_path = source.directory.join("installed/.nara-installation.json");
    let mut bytes = fs::read(&receipt_path).unwrap();
    bytes.push(b'\n');
    let expected_receipt = ContentDigest::of_bytes(&bytes);
    assert_ne!(expected_receipt, original_digest);
    fs::write(&receipt_path, &bytes).unwrap();
    let root = parent(&source)
        .open_directory_exact(&RelativePath::new("installed").unwrap())
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
    assert!(!source.directory.join("installed").exists());
    assert!(!fs::read_dir(&source.directory).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".nara-tool-stage-")
    }));
}

#[test]
fn conflicting_destination_is_never_overwritten_or_claimed() {
    let source = Release::new();
    let plan = plan(&source);
    fs::create_dir(source.directory.join("installed")).unwrap();
    fs::write(source.directory.join("installed/keep.txt"), "unowned").unwrap();
    assert!(matches!(
        plan.publish(),
        EditorSourceToolInstallationOutcome::Failed { .. }
    ));
    assert_eq!(
        fs::read(source.directory.join("installed/keep.txt")).unwrap(),
        b"unowned"
    );
    assert!(!source.directory.join("installed/nara-tool.json").exists());
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
    let EditorSourceToolInstallationOutcome::Incomplete { error, orphan, .. } = outcome else {
        panic!("unready published candidate must retain actual custody");
    };
    assert!(matches!(
        error,
        EditorSourceToolInstallationError::Receipt("injected before publish")
    ));
    let DirectoryPublicationOutcome::Published(_) = orphan.publication() else {
        panic!("the candidate directory was already published");
    };
    assert_eq!(orphan.pending_files(), 4);
    assert!(source.directory.join("installed").exists());
    assert!(
        !source
            .directory
            .join("installed/.nara-installation.json")
            .exists()
    );
    assert_eq!(orphan.members().len(), 4);
    assert!(
        orphan
            .directory()
            .unwrap()
            .open_directory(&RelativePath::new("samples").unwrap())
            .is_ok()
    );
    drop(orphan);
    assert!(source.directory.join("installed/Tool.dll").is_file());
}

#[test]
fn post_publication_member_replacement_preserves_published_evidence_and_original_custody() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| Ok(()),
        |_, _| {
            fs::rename(
                source.directory.join("installed/Tool.dll"),
                source.directory.join("installed/original.dll"),
            )
            .unwrap();
            fs::write(
                source.directory.join("installed/Tool.dll"),
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
    assert!(source.directory.join("installed/original.dll").is_file());
    assert!(installation.verify().is_err());
}

#[test]
fn retained_members_prevent_directory_rename_on_the_qualified_windows_filesystem() {
    let source = Release::new();
    let installation = published(plan(&source).publish());
    let error = fs::rename(
        source.directory.join("installed"),
        source.directory.join("moved-installation"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(source.directory.join("installed/Tool.dll").is_file());
    assert!(!source.directory.join("moved-installation").exists());
    installation.verify().unwrap();
}

#[test]
fn undeclared_payload_prevents_ready_commit_and_survives_candidate_retirement() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| {
            fs::write(source.directory.join("installed/unknown.dll"), b"unowned").unwrap();
            Ok(())
        },
        |_, _| Ok(()),
    );
    let EditorSourceToolInstallationOutcome::Incomplete { orphan, .. } = outcome else {
        panic!("undeclared payload must prevent ready publication");
    };
    assert!(
        !source
            .directory
            .join("installed/.nara-installation.json")
            .exists()
    );
    assert_eq!(orphan.pending_files(), 4);
    fs::write(
        source.directory.join("installed/samples/readme.txt"),
        b"user edit after failure",
    )
    .unwrap();
    drop(orphan);
    assert_eq!(
        fs::read(source.directory.join("installed/unknown.dll")).unwrap(),
        b"unowned"
    );
    assert_eq!(
        fs::read(source.directory.join("installed/samples/readme.txt")).unwrap(),
        b"user edit after failure"
    );
}

#[test]
fn post_ready_undeclared_member_requires_reconciliation_without_losing_publication() {
    let source = Release::new();
    let outcome = plan(&source).publish_with(
        |_| Ok(()),
        |_, _| {
            fs::write(source.directory.join("installed/unknown.dll"), b"unowned").unwrap();
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
    let root = source.directory.join("installed");
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
                source.directory.join("installed/.nara-installation.json"),
                b"unowned receipt",
            )
            .unwrap();
            Ok(())
        },
        |_, _| Ok(()),
    );
    let EditorSourceToolInstallationOutcome::Incomplete { orphan, .. } = outcome else {
        panic!("receipt collision must refuse the ready commit");
    };
    assert!(orphan.pending_files() > 0);
    drop(orphan);
    assert_eq!(
        fs::read(source.directory.join("installed/.nara-installation.json")).unwrap(),
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
            parent(&source),
            RelativeComponent::new("installed").unwrap(),
            candidate
        ),
        Err(EditorSourceToolInstallationError::ReservedPath)
    ));
    assert!(!source.directory.join("installed").exists());
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
            let path = source.directory.join("installed/.nara-installation.json");
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
