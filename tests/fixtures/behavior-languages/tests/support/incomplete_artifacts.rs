//! Native access denial and public recovery of an actually published, empty artifact.

use std::{
    ffi::c_void,
    fs, io,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::Path,
    ptr::{addr_of_mut, null_mut},
    time::{Duration, Instant},
};

use nara_engine::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, FsError, HostCapabilityOptions,
        RelativeComponent, TrustMode,
    },
    project_host::{
        EditorProjectSession, EditorSourceToolInstallationError,
        EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
        PreparedEditorSourceToolRelease, open_explicit_product_project_root,
    },
};
use nara_tooling::{
    EditorSourceToolArtifactOutcome, EditorSourceToolArtifactPhase,
    EditorSourceToolArtifactPublication, EditorSourceToolArtifactView,
    EditorSourceToolReclamationOutcome, EditorSourceToolReclamationPhase,
    EditorSourceToolReclamationRecordPhase, EditorSourceToolSelectionPhase,
};
use serde_json::json;
use windows_sys::Win32::{
    Foundation::{ERROR_ACCESS_DENIED, LocalFree},
    Security::{
        ACCESS_DENIED_ACE, ACE_HEADER, ACL,
        Authorization::{
            DENY_ACCESS, EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE,
            SE_FILE_OBJECT, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID,
            TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
        },
        CONTAINER_INHERIT_ACE, CreateWellKnownSid, DACL_SECURITY_INFORMATION, DeleteAce, EqualSid,
        GetAce, INHERIT_ONLY_ACE, INHERITED_ACE, PROTECTED_DACL_SECURITY_INFORMATION,
        SECURITY_MAX_SID_SIZE, WinWorldSid,
    },
    Storage::FileSystem::{
        FILE_ADD_FILE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    },
};

use super::authoring_project::{Project, assert_history, close, schema_bundle};

const STORE: &str = ".nara/source-tools/artifacts";
const DESTINATION: &str = ".nara/source-tools/artifacts/incomplete";
const INVENTORY: &str = ".nara/source-tools/artifacts.json";
const RECEIPT: &str = ".nara-installation.json";
const TIMEOUT: Duration = Duration::from_secs(30);

#[test]
fn public_incomplete_publication_records_empty_reclamation_and_survives_fresh_open() {
    let (_source, release) = finite_release();
    let manifest = release.manifest_digest();
    let project = Project::new(
        &schema_bundle(release.schema_owner()),
        &super::source(),
        "json",
    );
    let scene_bytes = fs::read(project.scene_path()).unwrap();
    let project_copy = project.root().join("assets/project-copy.txt");
    fs::write(&project_copy, b"Project-owned content must survive.\n").unwrap();
    let store = project.root().join(STORE);
    fs::create_dir_all(&store).unwrap();
    let root = project_root(project.root());
    let plan = EditorSourceToolInstallationPlan::new(
        root.share(),
        RelativeComponent::new("incomplete").unwrap(),
        release,
    )
    .unwrap();
    let mut denial = DenyChildFiles::install(&store).unwrap();
    // INHERIT_ONLY leaves the store's existing grants effective.
    let unknown = store.join("unknown-sibling.txt");
    fs::write(&unknown, b"Unknown sibling must survive.\n").unwrap();
    let publication = match plan.publish() {
        EditorSourceToolInstallationOutcome::Incomplete {
            plan,
            error,
            publication,
        } => {
            assert!(
                matches!(
                    error,
                    EditorSourceToolInstallationError::Member {
                        ref member,
                        source: FsError::Io { ref source, .. },
                    } if member == "nara-tool.json"
                        && source.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32)
                ),
                "Expected actual first-member access denial: {error:?}"
            );
            drop(plan);
            publication
        }
        other => panic!("Expected a real incomplete directory publication: {other:?}"),
    };
    assert_eq!(publication.project_path(), DESTINATION);
    assert!(publication.members().is_empty());
    let artifact = *publication.id();
    assert_ne!(artifact, [0; 32]);
    let native = project.root().join(DESTINATION);
    assert_eq!(fs::read_dir(&native).unwrap().count(), 0);
    assert!(!native.join(RECEIPT).exists());
    let denied_probe = native.join("denied-probe.txt");
    assert_eq!(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&denied_probe)
            .unwrap_err()
            .raw_os_error(),
        Some(ERROR_ACCESS_DENIED as i32),
    );
    let original_directory = project_root(&native);
    assert!(
        publication
            .directory()
            .same_object(&original_directory)
            .unwrap()
    );
    denial.remove_from_child(&native).unwrap();
    drop(denial);
    assert!(
        original_directory
            .same_object(&project_root(&native))
            .unwrap()
    );
    // Prove that access was repaired on the same object, without replacing the directory.
    fs::write(&denied_probe, b"Access restored.\n").unwrap();
    fs::remove_file(&denied_probe).unwrap();

    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let document_basis = editor.document_query_basis(document).unwrap();
    settle_selection(&mut editor);
    let before = settle_artifacts(&mut editor);
    assert!(before.artifacts.is_empty());
    assert!(before.reclamations.is_empty());
    assert!(!project.root().join(INVENTORY).exists());
    let prepared = editor
        .prepare_source_tool_incomplete_artifact_registration(&before.basis, publication)
        .unwrap();
    let expected = prepared.artifacts();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].id, artifact);
    assert_eq!(expected[0].manifest, manifest);
    assert_eq!(expected[0].directory.as_ref(), DESTINATION);
    assert_eq!(
        expected[0].publication,
        EditorSourceToolArtifactPublication::Incomplete,
    );
    assert!(expected[0].members.is_empty());
    assert_eq!(editor.source_tool_artifact_view(), before);
    assert!(!project.root().join(INVENTORY).exists());
    let registration = editor.commit_source_tool_artifacts(prepared).unwrap();
    let registered = settle_artifacts(&mut editor);
    assert_eq!(registered.artifacts, expected);
    let operation = registered.operation.as_ref().unwrap();
    assert_eq!(operation.attempt, registration);
    assert_eq!(
        operation.outcome,
        Some(EditorSourceToolArtifactOutcome::Registered)
    );
    let registered_bytes = fs::read(project.root().join(INVENTORY)).unwrap();
    let selection = editor.source_tool_selection_view();
    let review = editor
        .prepare_source_tool_reclamation(
            &registered.basis,
            &selection.basis,
            root.share(),
            &artifact,
        )
        .unwrap();
    assert!(review.creation_custody());
    let intent = review.record().clone();
    assert_eq!(intent.artifact, artifact);
    assert_eq!(intent.phase, EditorSourceToolReclamationRecordPhase::Intent);
    assert!(intent.members.is_empty());
    assert_eq!(
        fs::read(project.root().join(INVENTORY)).unwrap(),
        registered_bytes,
    );
    let attempt = editor.confirm_source_tool_reclamation(review).unwrap();
    let pending = editor.source_tool_reclamation_view();
    assert_eq!(
        pending.phase,
        EditorSourceToolReclamationPhase::PublishingIntent
    );
    assert_eq!(pending.record.as_ref(), Some(&intent));
    assert!(pending.record_publication_pending);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let view = editor.source_tool_reclamation_view();
        if view.phase == EditorSourceToolReclamationPhase::Terminal
            && !view.physical_work_pending
            && !view.record_publication_pending
        {
            assert_eq!(view.attempt.as_ref(), Some(&attempt));
            assert_eq!(
                view.outcome,
                Some(EditorSourceToolReclamationOutcome::Complete)
            );
            assert!(view.error.is_none(), "{view:?}");
            let mut results = intent.clone();
            results.phase = EditorSourceToolReclamationRecordPhase::Results;
            assert_eq!(view.record, Some(results));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Reclamation did not settle: {view:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let completed = settle_artifacts(&mut editor);
    assert_eq!(completed.artifacts, expected);
    assert_eq!(completed.reclamations.len(), 1);
    let mut results = intent;
    results.phase = EditorSourceToolReclamationRecordPhase::Results;
    assert_eq!(completed.reclamations[0], results);
    let durable = fs::read(project.root().join(INVENTORY)).unwrap();
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        document_basis
    );
    assert_history(&editor, document, 0, 0);
    assert_preserved(&project, &original_directory, &scene_bytes);
    assert_no_execution(&editor);
    close(&mut editor);
    drop(editor);
    drop(root);
    drop(original_directory);

    let mut reopened = project.open();
    settle_selection(&mut reopened);
    let observed = settle_artifacts(&mut reopened);
    assert_eq!(observed.artifacts, expected);
    assert_eq!(observed.reclamations, vec![results]);
    assert!(matches!(
        reopened.prepare_retained_source_tool_artifact_registration(&observed.basis, &artifact),
        Err(nara_tooling::EditorSourceToolArtifactError::Unregistered)
    ));
    assert_eq!(fs::read(project.root().join(INVENTORY)).unwrap(), durable);
    let fresh = reopened.source_tool_reclamation_view();
    assert!(fresh.attempt.is_none());
    assert!(fresh.outcome.is_none());
    assert!(!fresh.physical_work_pending && !fresh.record_publication_pending);
    let document = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    assert_history(&reopened, document, 0, 0);
    assert_preserved(&project, &project_root(&native), &scene_bytes);
    assert_no_execution(&reopened);
    close(&mut reopened);
}

fn finite_release() -> (tempfile::TempDir, PreparedEditorSourceToolRelease) {
    let source = tempfile::tempdir().unwrap();
    let catalog = super::catalog().to_json_string().unwrap().into_bytes();
    let mut files = Vec::new();
    for (path, bytes) in [
        ("catalog.json", catalog),
        (
            "Tool.dll",
            b"Data-only release; this payload must never execute.".to_vec(),
        ),
    ] {
        fs::write(source.path().join(path), &bytes).unwrap();
        let digest = ContentDigest::of_bytes(&bytes);
        files.push(json!({
            "path": path, "kind": "tool", "length": digest.length(),
            "blake3": digest.as_bytes().iter()
                .map(|byte| format!("{byte:02x}")).collect::<String>(),
        }));
    }
    let manifest = serde_json::to_vec(&json!({
        "format_version": 1, "package": "trial.incomplete", "release": "1",
        "host_target": "x86_64-pc-windows-msvc", "authority": "trusted_process",
        "protocol_version": 1, "owner": "trial.behaviors",
        "component": "trial.behavior.Label", "catalog": "catalog.json",
        "entry": "Tool.dll", "requires": [], "files": files,
    }))
    .unwrap();
    fs::write(source.path().join("nara-tool.json"), &manifest).unwrap();
    let release = PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(source.path()).unwrap(),
        ContentDigest::of_bytes(&manifest),
    )
    .unwrap();
    (source, release)
}

fn project_root(path: &Path) -> DirectoryCapability {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap();
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn settle_artifacts(editor: &mut EditorProjectSession) -> EditorSourceToolArtifactView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let view = editor.source_tool_artifact_view();
        if !view.physical_work_pending
            && view
                .operation
                .as_ref()
                .is_some_and(|operation| operation.outcome.is_some())
        {
            assert_eq!(view.phase, EditorSourceToolArtifactPhase::Idle, "{view:?}");
            assert!(view.error.is_none(), "{view:?}");
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Artifact inventory did not settle: {view:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_selection(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let view = editor.source_tool_selection_view();
        if view
            .operation
            .as_ref()
            .is_some_and(|operation| operation.outcome.is_some())
        {
            assert_eq!(view.phase, EditorSourceToolSelectionPhase::Idle, "{view:?}");
            assert!(view.error.is_none(), "{view:?}");
            assert!(view.installed.is_empty());
            assert!(view.desired.is_none() && view.active_origin.is_none());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Selection did not settle: {view:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_no_execution(editor: &EditorProjectSession) {
    let view = editor.source_tool_view();
    assert!(
        view.active.is_none() && !view.probing && !view.working && !view.retiring,
        "{view:?}",
    );
    assert!(view.attempt.is_none() && view.request.is_none());
    assert!(editor.source_tool_review().is_none());
    assert!(editor.source_tool_selection_view().installed.is_empty());
}

fn assert_preserved(project: &Project, original: &DirectoryCapability, scene: &[u8]) {
    let native = project.root().join(DESTINATION);
    assert!(original.same_object(&project_root(&native)).unwrap());
    assert_eq!(fs::read_dir(&native).unwrap().count(), 0);
    assert!(!native.join(RECEIPT).exists());
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene);
    assert_eq!(
        fs::read(project.root().join("assets/project-copy.txt")).unwrap(),
        b"Project-owned content must survive.\n",
    );
    assert_eq!(
        fs::read(project.root().join(STORE).join("unknown-sibling.txt")).unwrap(),
        b"Unknown sibling must survive.\n",
    );
    assert!(
        !project
            .root()
            .join(".nara/source-tools/selection.json")
            .exists()
    );
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: These buffers come exclusively from ACL APIs documented to use LocalFree.
        unsafe { LocalFree(self.0) };
    }
}

struct NativeDacl {
    name: Vec<u16>,
    _descriptor: LocalAllocation,
    acl: *mut ACL,
}

impl NativeDacl {
    fn read(path: &Path) -> io::Result<Self> {
        let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor = null_mut();
        let mut acl = null_mut();
        // SAFETY: The name is terminated; both output pointers remain live for this call.
        win32(unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut acl,
                null_mut(),
                &mut descriptor,
            )
        })?;
        let descriptor = LocalAllocation(descriptor);
        if acl.is_null() {
            return Err(io::Error::other(
                "The fixture directory must have an existing DACL",
            ));
        }
        Ok(Self {
            name,
            _descriptor: descriptor,
            acl,
        })
    }

    fn apply(&self, acl: *const ACL, flags: u32) -> io::Result<()> {
        // SAFETY: The name and API-owned ACL remain live throughout this synchronous call.
        win32(unsafe {
            SetNamedSecurityInfoW(
                self.name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | flags,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        })
    }
}

struct DenyChildFiles {
    original: NativeDacl,
}

impl DenyChildFiles {
    fn install(parent: &Path) -> io::Result<Self> {
        let original = NativeDacl::read(parent)?;
        let mut sid = world_sid()?;
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ADD_FILE,
            grfAccessMode: DENY_ACCESS,
            grfInheritance: CONTAINER_INHERIT_ACE | INHERIT_ONLY_ACE,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
                ptstrName: sid.as_mut_ptr().cast(),
            },
        };
        let mut merged = null_mut();
        // SAFETY: The SID and original descriptor outlive the merge; the result is LocalFree-owned.
        win32(unsafe { SetEntriesInAclW(1, &entry, original.acl, &mut merged) })?;
        let allocation = LocalAllocation(merged.cast());
        let guard = Self { original };
        guard.original.apply(merged, 0)?;
        drop(allocation);
        Ok(guard)
    }

    fn remove_from_child(&mut self, child: &Path) -> io::Result<()> {
        let child = NativeDacl::read(child)?;
        let mut sid = world_sid()?;
        let mut removed = 0;
        // SAFETY: The descriptor owns this nonnull native ACL, including every indexed ACE.
        unsafe {
            for index in (0..u32::from((*child.acl).AceCount)).rev() {
                let mut ace = null_mut();
                if GetAce(child.acl, index, &mut ace) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let header = &*ace.cast::<ACE_HEADER>();
                // ACCESS_DENIED_ACE_TYPE is 0x01; inspect only that native ACE layout.
                if header.AceType == 0x01
                    && u32::from(header.AceFlags) & INHERITED_ACE != 0
                    && usize::from(header.AceSize) >= size_of::<ACCESS_DENIED_ACE>() + 8
                {
                    let denied = &mut *ace.cast::<ACCESS_DENIED_ACE>();
                    if denied.Mask == FILE_ADD_FILE
                        && EqualSid(
                            addr_of_mut!(denied.SidStart).cast(),
                            sid.as_mut_ptr().cast(),
                        ) != 0
                    {
                        if DeleteAce(child.acl, index) == 0 {
                            return Err(io::Error::last_os_error());
                        }
                        removed += 1;
                    }
                }
            }
        }
        assert_eq!(
            removed, 1,
            "The published directory must inherit the exact deny ACE"
        );
        // Protect the repaired child against immediately inheriting the same deny again.
        child.apply(child.acl, PROTECTED_DACL_SECURITY_INFORMATION)?;
        self.original.apply(self.original.acl, 0)
    }
}

impl Drop for DenyChildFiles {
    fn drop(&mut self) {
        if let Err(error) = self.original.apply(self.original.acl, 0) {
            eprintln!("Could not restore the temporary artifact store DACL: {error}");
        }
    }
}

fn world_sid() -> io::Result<[u32; SECURITY_MAX_SID_SIZE as usize / 4]> {
    let mut sid = [0; SECURITY_MAX_SID_SIZE as usize / 4];
    let mut length = SECURITY_MAX_SID_SIZE;
    // SAFETY: The DWORD-aligned buffer has the documented maximum SID capacity.
    if unsafe {
        CreateWellKnownSid(
            WinWorldSid,
            null_mut(),
            sid.as_mut_ptr().cast(),
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(sid)
}

fn win32(status: u32) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}
