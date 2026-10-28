//! Real installation, selection and publication through held executor return boundaries.

use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara_fs::HostCapabilityOptions;
use nara_tooling::{EditorSourceToolInstallationRequest, EditorSourceToolSelectionOutcome};

use super::*;
use crate::project_host::{
    EditorProjectCloseProgress,
    runtime::editor::source_tools::{
        EditorSourceToolInstallation, EditorSourceToolInstallationOutcome,
        EditorSourceToolInstallationPlan, PreparedEditorSourceToolRelease,
        release::tests::Release,
        tests::fixture::{Project, wait},
    },
};

const WAIT: Duration = Duration::from_secs(15);

pub(super) struct IoGate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    fail_cleanup: bool,
}

impl IoGate {
    pub(super) fn wait_after_io(&self) {
        let _ = self.entered.send(());
        let _ = self
            .release
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .recv_timeout(WAIT * 2);
    }
}

impl Drop for IoGate {
    fn drop(&mut self) {
        assert!(
            !self.fail_cleanup,
            "injected template capture cleanup failure"
        );
    }
}

pub(super) struct ReleaseGate(mpsc::Sender<()>);

impl Drop for ReleaseGate {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

pub(super) fn root(project: &Project) -> DirectoryCapability {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(&project.root)
        .unwrap();
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn registered(
    editor: &mut EditorProjectSession,
    project: &Project,
) -> EditorSourceToolInstallation {
    wait(editor, |editor| {
        !editor.source_tool_view().has_pending_work()
    });
    let source = Release::new();
    let captured = PreparedEditorSourceToolRelease::capture(
        &source.root(TrustMode::Untrusted),
        source.write_manifest(),
    )
    .unwrap();
    register_release(editor, project, captured, "text-template")
}

pub(super) fn register_release(
    editor: &mut EditorProjectSession,
    project: &Project,
    captured: PreparedEditorSourceToolRelease,
    name: &str,
) -> EditorSourceToolInstallation {
    let plan = EditorSourceToolInstallationPlan::new(
        root(project),
        RelativeComponent::new(name).unwrap(),
        captured,
    )
    .unwrap();
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        outcome => panic!("expected actual fixed-store publication: {outcome:?}"),
    };
    let basis = editor.source_tool_selection_view().basis;
    let prepared = editor
        .prepare_source_tool_installations(
            &basis,
            root(project),
            &[EditorSourceToolInstallationRequest {
                directory: installation.project_path().into(),
                receipt: installation.receipt_digest(),
                requirements: Vec::new(),
            }],
        )
        .unwrap();
    let attempt = editor.register_source_tool_installations(prepared).unwrap();
    wait(editor, |editor| {
        !editor.source_tool_view().has_pending_work()
    });
    let selection = editor.source_tool_selection_view();
    let operation = selection.operation.unwrap();
    assert_eq!(operation.attempt, attempt);
    assert_eq!(
        operation.outcome,
        Some(EditorSourceToolSelectionOutcome::Registered)
    );
    installation
}

pub(super) fn hold_import(editor: &mut EditorProjectSession) -> (mpsc::Receiver<()>, ReleaseGate) {
    let (entered, entrance) = mpsc::channel();
    let (release, released) = mpsc::channel();
    editor.source_tools.template_import.next_gate = Some(Arc::new(IoGate {
        entered,
        release: Mutex::new(released),
        fail_cleanup: false,
    }));
    (entrance, ReleaseGate(release))
}

fn verify_actual_return(fail_cleanup: bool) {
    let project = Project::new();
    let mut editor = project.open();
    let installation = registered(&mut editor, &project);
    let basis = editor.source_tool_selection_view().basis;
    let prepared = editor
        .prepare_source_tool_template_import(
            &basis,
            root(&project),
            installation.receipt().installation_id(),
            "samples/readme.txt",
            "project-copy.txt",
        )
        .unwrap();
    assert_eq!(prepared.bytes(), b"editable sample");
    let (entered, entrance) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let control = ReleaseGate(release);
    editor.source_tools.template_import.next_gate = Some(Arc::new(IoGate {
        entered,
        release: Mutex::new(released),
        fail_cleanup,
    }));
    let attempt = editor.commit_source_tool_template_import(prepared).unwrap();
    entrance.recv_timeout(WAIT).unwrap();
    let returned = editor
        .source_tools
        .template_import
        .pending
        .as_ref()
        .unwrap()
        .task
        .as_ref()
        .unwrap()
        .return_observer();
    assert_eq!(
        fs::read(project.root.join("project-copy.txt")).unwrap(),
        b"editable sample"
    );
    editor.cancel_source_tool_template_import(&attempt).unwrap();
    for _ in 0..3 {
        let started = Instant::now();
        editor.advance_source_tools();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(returned.state(), TaskReturnState::Pending);
        let view = editor.source_tool_template_import_view();
        assert!(view.physical_work_pending);
        assert_eq!(view.outcome, None);
        assert!(!view.cleanup_failed);
    }
    drop(control);
    wait(&mut editor, |editor| {
        !editor
            .source_tool_template_import_view()
            .physical_work_pending
    });
    let view = editor.source_tool_template_import_view();
    assert_eq!(view.attempt, Some(attempt));
    assert!(matches!(view.outcome, Some(Outcome::Imported { .. })));
    assert_eq!(view.cleanup_failed, fail_cleanup);
    assert_eq!(
        returned.state(),
        if fail_cleanup {
            TaskReturnState::CleanupFailed
        } else {
            TaskReturnState::Returned
        }
    );
    assert_eq!(
        fs::read(project.root.join("project-copy.txt")).unwrap(),
        b"editable sample"
    );
    assert_eq!(
        editor.exclude_project_source_aliases("project-copy.txt"),
        Ok(())
    );
    let deadline = Instant::now() + WAIT;
    loop {
        match editor.advance_close() {
            EditorProjectCloseProgress::Complete(receipt) => {
                assert_eq!(receipt.task_shutdown().has_cleanup_failure(), fail_cleanup);
                break;
            }
            progress => assert!(
                Instant::now() < deadline,
                "actual import return must close: {progress:?}"
            ),
        }
        thread::sleep(Duration::from_millis(1));
    }
    drop(editor);
}

#[test]
fn cancellation_after_copy_publication_waits_for_actual_return_and_preserves_receipt() {
    verify_actual_return(false);
}

#[test]
fn accepted_copy_and_failed_executor_cleanup_remain_independent_public_facts() {
    verify_actual_return(true);
}

#[test]
fn namespace_parent_replacement_after_review_refuses_the_copy() {
    let project = Project::new();
    project.run(|editor| {
        let installation = registered(editor, &project);
        fs::create_dir(project.root.join("copies")).unwrap();
        let basis = editor.source_tool_selection_view().basis;
        let prepared = editor
            .prepare_source_tool_template_import(
                &basis,
                root(&project),
                installation.receipt().installation_id(),
                "samples/readme.txt",
                "copies/copy.txt",
            )
            .unwrap();
        fs::rename(
            project.root.join("copies"),
            project.root.join("original-copies"),
        )
        .unwrap();
        fs::create_dir(project.root.join("copies")).unwrap();
        editor.commit_source_tool_template_import(prepared).unwrap();
        wait(editor, |editor| {
            !editor
                .source_tool_template_import_view()
                .physical_work_pending
        });
        assert_eq!(
            editor.source_tool_template_import_view().outcome,
            Some(Outcome::Failed(Error::Changed))
        );
        assert!(!project.root.join("copies/copy.txt").exists());
        assert!(!project.root.join("original-copies/copy.txt").exists());
    });
}

#[test]
fn equal_byte_installation_directory_replacement_cannot_authorize_the_original_copy() {
    let project = Project::new();
    project.run(|editor| {
        let installation = registered(editor, &project);
        let installation_id = *installation.receipt().installation_id();
        let source_path = installation.project_path().to_owned();
        let members = installation
            .members()
            .iter()
            .map(|member| {
                (
                    member.path().to_owned(),
                    fs::read(project.root.join(&source_path).join(member.path())).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        // The graph retains exact facts; dropping creation custody permits this native rename.
        drop(installation);
        let basis = editor.source_tool_selection_view().basis;
        let prepared = editor
            .prepare_source_tool_template_import(
                &basis,
                root(&project),
                &installation_id,
                "samples/readme.txt",
                "copy.txt",
            )
            .unwrap();
        let prior = project
            .root
            .join(".nara/source-tools/artifacts/original-template");
        fs::rename(project.root.join(&source_path), &prior).unwrap();
        for (path, bytes) in &members {
            let replacement = project.root.join(&source_path).join(path);
            fs::create_dir_all(replacement.parent().unwrap()).unwrap();
            fs::write(&replacement, bytes).unwrap();
            assert_eq!(fs::read(prior.join(path)).unwrap(), *bytes);
        }
        assert_eq!(editor.source_tool_selection_view().basis, basis);
        editor.commit_source_tool_template_import(prepared).unwrap();
        wait(editor, |editor| {
            !editor
                .source_tool_template_import_view()
                .physical_work_pending
        });
        assert_eq!(
            editor.source_tool_template_import_view().outcome,
            Some(Outcome::Failed(Error::Changed))
        );
        assert!(!project.root.join("copy.txt").exists());
    });
}

#[test]
fn destination_parent_requires_exact_stored_spelling_and_portable_separators() {
    let project = Project::new();
    project.run(|editor| {
        let installation = registered(editor, &project);
        fs::create_dir(project.root.join("copies")).unwrap();
        let basis = editor.source_tool_selection_view().basis;
        for destination in [
            "COPIES/copy.txt",
            "copies\\copy.txt",
            ".nara\\source-tools\\artifacts\\copy.txt",
        ] {
            assert!(
                matches!(
                    editor.prepare_source_tool_template_import(
                        &basis,
                        root(&project),
                        installation.receipt().installation_id(),
                        "samples/readme.txt",
                        destination,
                    ),
                    Err(Error::InvalidPath)
                ),
                "{destination} must not create an aliased project copy"
            );
        }
        assert!(!project.root.join("copies/copy.txt").exists());
        assert!(editor.source_tool_template_import_view().attempt.is_none());
    });
}
