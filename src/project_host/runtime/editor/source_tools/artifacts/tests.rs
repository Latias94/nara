//! Actual inventory publication and Session custody; no deletion/reclamation qualification.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(15);

pub(super) struct IoGate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    fail_cleanup: bool,
}

impl IoGate {
    fn new() -> (Arc<Self>, IoControl) {
        Self::with_cleanup_failure(false)
    }

    fn with_cleanup_failure(fail_cleanup: bool) -> (Arc<Self>, IoControl) {
        let (entered, entrance) = mpsc::channel();
        let (release, released) = mpsc::channel();
        (
            Arc::new(Self {
                entered,
                release: Mutex::new(released),
                fail_cleanup,
            }),
            IoControl { entrance, release },
        )
    }

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
            "injected artifact task capture cleanup failure"
        );
    }
}

struct IoControl {
    entrance: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

impl IoControl {
    fn entered(&self) {
        self.entrance
            .recv_timeout(WAIT)
            .expect("actual artifact IO must reach its return gate");
    }

    fn release(&self) {
        let _ = self.release.send(());
    }
}

impl Drop for IoControl {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
mod session {
    use std::{fs, sync::Arc, thread, time::Instant};

    use nara_fs::{
        CapabilityRights, HostCapabilityOptions, RelativeComponent, RelativePath, TrustMode,
    };
    use nara_scene::{
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    };
    use nara_tasks::TaskReturnState;
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorPersistenceAction,
        EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorSourceToolArtifactObservation as Observation, EditorSourceToolArtifactPublication,
    };

    use super::super::*;
    use super::{Duration, IoGate};
    use crate::project_host::{
        EditorProjectCloseProgress,
        file_publication::{FilePublicationFailureStage, FilePublicationRejection},
        runtime::editor::{
            source_admission::SourceAdmissionError,
            source_tools::{
                EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
                PreparedEditorSourceToolRelease,
                release::tests::Release,
                tests::fixture::{Project, wait},
            },
        },
    };

    const NAME: &str = "stats-one";
    const SAMPLE: &str = "samples/readme.txt";
    const MOVED: &str = "assets/retained-template.txt";

    fn loaded(editor: &mut EditorProjectSession) -> EditorSourceToolArtifactView {
        wait(editor, |editor| {
            let view = editor.source_tool_artifact_view();
            matches!(view.phase, Phase::Idle | Phase::Unavailable)
                && !view.physical_work_pending
                && view
                    .operation
                    .as_ref()
                    .is_some_and(|operation| operation.outcome.is_some())
        });
        editor.source_tool_artifact_view()
    }

    fn install(project: &Project) -> Arc<EditorSourceToolInstallation> {
        use std::os::windows::fs::OpenOptionsExt;

        let release = Release::new();
        let digest = release.write_manifest();
        let captured =
            PreparedEditorSourceToolRelease::capture(&release.root(TrustMode::Untrusted), digest)
                .unwrap();
        let root = DirectoryCapability::from_host_handle(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&project.root)
                .unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
        )
        .unwrap();
        let plan = EditorSourceToolInstallationPlan::new(
            root,
            RelativeComponent::new(NAME).unwrap(),
            captured,
        )
        .unwrap();
        match plan.publish() {
            EditorSourceToolInstallationOutcome::Published(installation) => Arc::new(installation),
            other => panic!("expected actual fixed-store installation: {other:?}"),
        }
    }

    fn completed(
        editor: &mut EditorProjectSession,
        attempt: &EditorSourceToolAttemptId,
        expected: Outcome,
    ) -> EditorSourceToolArtifactView {
        wait(editor, |editor| {
            let view = editor.source_tool_artifact_view();
            !view.physical_work_pending
                && view.operation.as_ref().is_some_and(|operation| {
                    &operation.attempt == attempt && operation.outcome.is_some()
                })
        });
        let view = editor.source_tool_artifact_view();
        assert_eq!(
            view.operation.as_ref().unwrap().outcome.as_ref(),
            Some(&expected)
        );
        view
    }

    fn register(
        editor: &mut EditorProjectSession,
        installation: &Arc<EditorSourceToolInstallation>,
    ) -> EditorSourceToolArtifactView {
        let basis = editor.source_tool_artifact_view().basis;
        let prepared = editor
            .prepare_source_tool_artifact_registration(&basis, Arc::clone(installation))
            .unwrap();
        let expected = prepared.artifacts();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        let view = completed(editor, &attempt, Outcome::Registered);
        assert_eq!(view.artifacts, expected);
        assert_eq!(view.phase, Phase::Idle);
        view
    }

    fn sample(view: &EditorSourceToolArtifactView) -> Observation {
        view.artifacts[0]
            .members
            .iter()
            .find(|member| member.path.as_ref() == SAMPLE)
            .unwrap()
            .observation
    }

    fn observe(editor: &mut EditorProjectSession, id: &[u8; 32]) -> EditorSourceToolArtifactView {
        let basis = editor.source_tool_artifact_view().basis;
        let prepared = editor
            .prepare_source_tool_artifact_observation(&basis, id)
            .unwrap();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        completed(editor, &attempt, Outcome::Observed)
    }

    #[test]
    fn preparation_drop_and_view_are_read_only_and_confirmation_uses_original_basis() {
        let project = Project::new();
        project.run(|editor| {
            let before = loaded(editor);
            let installation = install(&project);
            let prepared = editor
                .prepare_source_tool_artifact_registration(&before.basis, Arc::clone(&installation))
                .unwrap();
            assert_eq!(editor.source_tool_artifact_view(), before);
            assert_eq!(
                prepared.artifacts()[0].directory.as_ref(),
                installation.project_path()
            );
            assert_eq!(
                prepared.artifacts()[0].publication,
                EditorSourceToolArtifactPublication::Ready {
                    receipt: installation.receipt_digest(),
                }
            );
            assert!(!project.root.join(ARTIFACTS_PATH).exists());
            drop(prepared);
            assert_eq!(editor.source_tool_artifact_view(), before);
            assert!(editor.source_tools.artifacts.created.is_empty());
            let stale = editor
                .prepare_source_tool_artifact_registration(&before.basis, Arc::clone(&installation))
                .unwrap();
            let registered = register(editor, &installation);
            let bytes = fs::read(project.root.join(ARTIFACTS_PATH)).unwrap();
            assert_eq!(
                editor
                    .commit_source_tool_artifacts(stale)
                    .unwrap_err()
                    .error(),
                &Error::StaleBasis
            );
            assert_eq!(editor.source_tool_artifact_view(), registered);
            assert_eq!(fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(), bytes);
            assert_eq!(sample(&registered), Observation::Present);
            assert!(editor.source_tools.active.is_none());
        });
    }

    #[test]
    fn another_actual_project_is_refused_without_publishing_inventory() {
        let foreign_project = Project::new();
        let mut foreign = None;
        foreign_project.run(|editor| {
            loaded(editor);
            foreign = Some(install(&foreign_project));
        });
        let foreign = foreign.unwrap();
        let project = Project::new();
        project.run(|editor| {
            let before = loaded(editor);
            assert!(matches!(
                editor
                    .prepare_source_tool_artifact_registration(&before.basis, Arc::clone(&foreign)),
                Err(Error::RootMismatch)
            ));
            assert_eq!(editor.source_tool_artifact_view(), before);
            assert!(!project.root.join(ARTIFACTS_PATH).exists());
        });
    }

    #[test]
    fn observation_is_finite_and_reports_changes_missing_and_failures_without_sticky_flags() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let installation = install(&project);
            register(editor, &installation);
            let id = installation.receipt().installation_id();
            let path = project.root.join(installation.project_path()).join(SAMPLE);
            fs::write(&path, b"changed template bytes").unwrap();
            assert_eq!(sample(&observe(editor, id)), Observation::Changed);
            fs::write(&path, b"editable sample").unwrap();
            assert_eq!(sample(&observe(editor, id)), Observation::Present);
            fs::rename(&path, project.root.join(MOVED)).unwrap();
            assert_eq!(sample(&observe(editor, id)), Observation::Missing);
            fs::write(&path, b"editable sample").unwrap();
            assert_eq!(sample(&observe(editor, id)), Observation::Changed);
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            assert_eq!(sample(&observe(editor, id)), Observation::Failed);
            fs::remove_dir(&path).unwrap();
            assert!(project.root.join(MOVED).exists());
            assert!(
                installation
                    .directory()
                    .open_file_exact(&RelativePath::new("Tool.dll").unwrap())
                    .is_ok()
            );
        });
    }

    #[test]
    fn live_registration_reserves_renamed_objects_and_existing_source_owners_are_checked() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let source = editor
                .source_project_root()
                .open_file_exact(&RelativePath::new("scenes/startup.scene.json").unwrap())
                .unwrap();
            assert_eq!(
                editor.require_unowned_artifact_object(&source),
                Err(Error::SourceOwned)
            );
            let installation = install(&project);
            register(editor, &installation);
            fs::rename(
                project.root.join(installation.project_path()).join(SAMPLE),
                project.root.join(MOVED),
            )
            .unwrap();
            let current = editor
                .source_project_root()
                .open_file_exact(&RelativePath::new(MOVED).unwrap())
                .unwrap();
            assert!(
                editor
                    .source_tools
                    .artifacts
                    .contains_source_object(&current)
                    .unwrap()
            );
            assert_eq!(
                editor.exclude_source_aliases(MOVED, &current),
                Err(SourceAdmissionError::PrivateArtifact)
            );
            assert!(
                !editor
                    .source_tools
                    .artifacts
                    .contains_source_object(&source)
                    .unwrap()
            );
        });
    }

    #[test]
    fn actual_failed_publication_keeps_candidate_cleanup_and_original_failure_stage() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let installation = install(&project);
            let registered = register(editor, &installation);
            let before = fs::read(project.root.join(ARTIFACTS_PATH)).unwrap();
            let host = editor.source_tools.artifacts.host.clone().unwrap();
            let control = host.try_with(|host| host.test_fail_after_write()).unwrap();
            let prepared = editor
                .prepare_source_tool_artifact_observation(
                    &registered.basis,
                    installation.receipt().installation_id(),
                )
                .unwrap();
            let expected = prepared.artifacts();
            let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
            wait(editor, |editor| {
                editor
                    .source_tool_artifact_view()
                    .operation
                    .as_ref()
                    .is_some_and(|operation| operation.outcome.is_some())
            });
            let failed = editor.source_tool_artifact_view();
            assert_eq!(failed.operation.as_ref().unwrap().attempt, attempt);
            assert_eq!(failed.error, Some(Error::Write));
            assert!(failed.physical_work_pending);
            let received = editor.source_tools.artifacts.received.as_ref().unwrap();
            assert_eq!(received._candidate.artifacts(), expected);
            assert!(matches!(
                &received._completed.result,
                Ok(io::IoResult::Refused {
                    _outcome: io::PublicationRefusal::Failed {
                        _stage: FilePublicationFailureStage::WriteTemporary
                    },
                    ..
                })
            ));
            assert_eq!(fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(), before);
            control.release();
            wait(editor, |editor| {
                !editor.source_tool_artifact_view().physical_work_pending
            });
        });
    }

    #[test]
    fn changed_binding_requires_explicit_reconcile_without_merging_candidate_into_disk_facts() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let installation = install(&project);
            let registered = register(editor, &installation);
            let prepared = editor
                .prepare_source_tool_artifact_observation(
                    &registered.basis,
                    installation.receipt().installation_id(),
                )
                .unwrap();
            let record = project.root.join(ARTIFACTS_PATH);
            let moved = project
                .root
                .join(".nara/source-tools/original-inventory.json");
            fs::rename(&record, &moved).unwrap();
            let disk = record::encode(&record::Inventory::default()).unwrap();
            fs::write(&record, &disk).unwrap();
            let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
            let failed = completed(editor, &attempt, Outcome::Failed(Error::Changed));
            assert_eq!(failed.phase, Phase::RequiresReconcile);
            assert!(matches!(
                editor.prepare_source_tool_artifact_observation(
                    &failed.basis,
                    installation.receipt().installation_id()
                ),
                Err(Error::RequiresReconcile)
            ));
            assert!(matches!(
                &editor
                    .source_tools
                    .artifacts
                    .received
                    .as_ref()
                    .unwrap()
                    ._completed
                    .result,
                Ok(io::IoResult::Refused {
                    _outcome: io::PublicationRefusal::Rejected {
                        _reason: FilePublicationRejection::TargetChanged,
                    },
                    ..
                })
            ));
            let reconcile = editor
                .reconcile_source_tool_artifacts(&failed.basis)
                .unwrap();
            let observed = completed(editor, &reconcile, Outcome::Reconciled);
            assert!(observed.artifacts.is_empty());
            assert_eq!(fs::read(&record).unwrap(), disk);
            assert!(editor.source_tools.artifacts.received.is_some());
            assert_eq!(editor.source_tools.artifacts.created.len(), 1);
            let original = installation
                .members()
                .iter()
                .find(|member| member.path() == SAMPLE)
                .unwrap();
            assert!(
                editor
                    .source_tools
                    .artifacts
                    .contains_source_object(original.file())
                    .unwrap()
            );
        });
    }

    #[test]
    fn cancelled_task_still_acknowledges_actual_publication_after_physical_return() {
        let project = Project::new();
        project.run(|editor| {
            let before = loaded(editor);
            let installation = install(&project);
            let prepared = editor
                .prepare_source_tool_artifact_registration(&before.basis, Arc::clone(&installation))
                .unwrap();
            let expected = prepared.artifacts();
            let (gate, control) = IoGate::new();
            editor.source_tools.artifacts.next_io_gate = Some(gate);
            let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
            control.entered();
            let task = editor
                .source_tools
                .artifacts
                .pending
                .as_ref()
                .unwrap()
                .task
                .as_ref()
                .unwrap();
            let returned = task.return_observer();
            assert!(task.cancel());
            for _ in 0..3 {
                editor.advance_source_tool_artifacts();
                let view = editor.source_tool_artifact_view();
                assert_eq!(view.phase, Phase::Publishing);
                assert_eq!(view.artifacts, before.artifacts);
                assert!(view.physical_work_pending);
                assert!(view.operation.as_ref().unwrap().outcome.is_none());
                assert_eq!(returned.state(), TaskReturnState::Pending);
            }
            let bytes = fs::read(project.root.join(ARTIFACTS_PATH)).unwrap();
            assert_eq!(record::decode(&bytes).unwrap().artifacts, expected);
            control.release();
            let completed = completed(editor, &attempt, Outcome::Registered);
            assert_eq!(completed.artifacts, expected);
            assert_eq!(returned.state(), TaskReturnState::Returned);
            assert!(matches!(
                editor
                    .source_tools
                    .artifacts
                    .received
                    .as_ref()
                    .unwrap()
                    ._completed
                    .result,
                Ok(io::IoResult::Published { accepted: true, .. })
            ));
        });
    }

    #[test]
    fn accepted_publication_and_actual_capture_cleanup_failure_are_both_observable() {
        let project = Project::new();
        let mut editor = project.open();
        let before = loaded(&mut editor);
        let installation = install(&project);
        let prepared = editor
            .prepare_source_tool_artifact_registration(&before.basis, Arc::clone(&installation))
            .unwrap();
        let expected = prepared.artifacts();
        let (gate, control) = IoGate::with_cleanup_failure(true);
        editor.source_tools.artifacts.next_io_gate = Some(gate);
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        control.entered();
        let returned = editor
            .source_tools
            .artifacts
            .pending
            .as_ref()
            .unwrap()
            .task
            .as_ref()
            .unwrap()
            .return_observer();
        assert_eq!(
            record::decode(&fs::read(project.root.join(ARTIFACTS_PATH)).unwrap())
                .unwrap()
                .artifacts,
            expected
        );
        assert_eq!(returned.state(), TaskReturnState::Pending);
        control.release();
        let published = completed(&mut editor, &attempt, Outcome::PublishedWithCleanupFailure);
        assert_eq!(published.artifacts, expected);
        assert_eq!(published.error, Some(Error::TaskCleanupFailed));
        assert_eq!(published.phase, Phase::RequiresReconcile);
        assert!(!published.physical_work_pending);
        assert_eq!(returned.state(), TaskReturnState::CleanupFailed);
        let received = editor.source_tools.artifacts.received.as_ref().unwrap();
        assert_eq!(
            received._completed.return_state,
            TaskReturnState::CleanupFailed
        );
        assert!(matches!(
            &received._completed.result,
            Ok(io::IoResult::Published { accepted: true, .. })
        ));
        for _ in 0..3 {
            editor.advance_source_tool_artifacts();
            assert_eq!(editor.source_tool_artifact_view(), published);
        }
        let deadline = Instant::now() + super::WAIT;
        loop {
            match editor.advance_close() {
                EditorProjectCloseProgress::Complete(receipt) => {
                    assert!(receipt.task_shutdown().is_complete());
                    assert!(receipt.task_shutdown().has_cleanup_failure());
                    assert!(!editor.source_tool_artifact_view().physical_work_pending);
                    break;
                }
                progress => assert!(
                    Instant::now() < deadline,
                    "failed artifact cleanup must physically close: {progress:?}"
                ),
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn unavailable_inventory_does_not_gate_ordinary_edit_save_reopen_or_close() {
        let project = Project::new();
        fs::create_dir_all(project.root.join(".nara/source-tools")).unwrap();
        fs::write(project.root.join(ARTIFACTS_PATH), b"invalid inventory").unwrap();
        project.run(|editor| {
            let before = loaded(editor);
            assert_eq!(before.phase, Phase::Unavailable);
            let document = editor.workspace.active_document().unwrap();
            let basis = editor.document_query_basis(document).unwrap();
            let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(SceneEntityId::new("ordinary-edit").unwrap()),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }]);
            let prepared = editor.prepare_authoring_edit(&basis, &patch).unwrap();
            assert!(matches!(editor.commit_authoring_edit(prepared), EditorActionOutcome::Workspace(report) if report.applied));
            let revision = editor.document_query_basis(document).unwrap().revision;
            for reopen in [false, true] {
                let action = if reopen {
                    EditorPersistenceAction::Reopen { target: editor.current_frame().reopen_target() }
                } else {
                    EditorPersistenceAction::Save { document, base_revision: revision }
                };
                let report = editor.dispatch_editor_actions(EditorActionBatch::new(
                    editor.current_frame().revision(), [EditorAction::Persistence(action)],
                )).unwrap();
                let [EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { attempt })] = report.outcomes() else {
                    panic!("ordinary persistence must be admitted with invalid inventory: {report:?}");
                };
                let attempt = *attempt;
                wait(editor, |editor| editor.persistence_view().attempt() == Some(attempt) && editor.persistence_view().result().is_some());
                assert!(matches!(editor.persistence_view().result(),
                    Some(EditorPersistenceResult::Saved { .. } | EditorPersistenceResult::Opened { .. })));
                let acknowledged = editor.dispatch_editor_actions(EditorActionBatch::new(
                    editor.current_frame().revision(),
                    [EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt })],
                )).unwrap();
                assert_eq!(acknowledged.outcomes(), &[
                    EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { attempt }),
                ]);
            }
            assert_eq!(editor.source_tool_artifact_view(), before);
            assert_eq!(fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(), b"invalid inventory");
            // Project::run drives actual Close and verifies Tasks and HostCustody return.
        });
    }
}
