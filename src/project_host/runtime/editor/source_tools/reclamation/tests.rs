//! Real fixed-store reclamation, publication failure and cancellation regressions.

use std::{
    sync::{Mutex, mpsc},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(15);

pub(super) struct RemovalGate {
    entered: mpsc::Sender<usize>,
    release: Mutex<mpsc::Receiver<()>>,
    after: usize,
}

impl RemovalGate {
    pub(super) fn after_member(&self, index: usize) {
        if index == self.after {
            let _ = self.entered.send(index);
            let _ = self
                .release
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .recv_timeout(WAIT * 2);
        }
    }
}

struct RemovalControl {
    entered: mpsc::Receiver<usize>,
    release: mpsc::Sender<()>,
}

impl Drop for RemovalControl {
    fn drop(&mut self) {
        let _ = self.release.send(());
    }
}

#[cfg(all(target_arch = "x86_64", target_env = "msvc"))]
mod session {
    use std::{fs, os::windows::fs::OpenOptionsExt, thread, time::Instant};

    use nara_fs::{
        CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativeComponent, TrustMode,
    };
    use nara_tooling::{
        EditorSourceToolArtifactOutcome as ArtifactOutcome,
        EditorSourceToolArtifactPhase as ArtifactPhase, EditorSourceToolArtifactView,
        EditorSourceToolInstallationRequest, EditorSourceToolSelectionOutcome as SelectionOutcome,
        EditorSourceToolSelectionPhase as SelectionPhase, EditorSourceToolWithdrawalMode,
    };

    use super::super::*;
    use super::{Duration, RemovalControl, RemovalGate, WAIT, mpsc};
    use crate::project_host::EditorProjectCloseProgress;
    use crate::project_host::runtime::editor::source_tools::{
        EditorSourceToolInstallation, EditorSourceToolInstallationOutcome,
        EditorSourceToolInstallationPlan, PreparedEditorSourceToolRelease,
        artifacts::{ARTIFACTS_PATH, ArtifactState},
        release::tests::Release,
        tests::fixture::{Project, wait},
    };

    const NAME: &str = "reclamation-one";
    const UNKNOWN: &str = "unknown-sibling.txt";

    fn writable_root(project: &Project) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&project.root)
                .unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
        )
        .unwrap()
    }

    fn quiet(editor: &mut EditorProjectSession) {
        wait(editor, |editor| {
            let artifacts = editor.source_tool_artifact_view();
            artifacts.phase == ArtifactPhase::Idle
                && !artifacts.physical_work_pending
                && editor.source_tool_selection_view().phase == SelectionPhase::Idle
                && editor.require_reclamation_quiet().is_ok()
        });
        assert!(editor.source_tools.active.is_none());
    }

    fn artifact_completed(
        editor: &mut EditorProjectSession,
        attempt: &EditorSourceToolAttemptId,
        expected: ArtifactOutcome,
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
        project: &Project,
    ) -> Arc<EditorSourceToolInstallation> {
        quiet(editor);
        let release = Release::new();
        let captured = PreparedEditorSourceToolRelease::capture(
            &release.root(TrustMode::Untrusted),
            release.write_manifest(),
        )
        .unwrap();
        let installation = match EditorSourceToolInstallationPlan::new(
            writable_root(project),
            RelativeComponent::new(NAME).unwrap(),
            captured,
        )
        .unwrap()
        .publish()
        {
            EditorSourceToolInstallationOutcome::Published(installation) => Arc::new(installation),
            other => panic!("expected an actual code-free installation: {other:?}"),
        };
        let basis = editor.source_tool_artifact_view().basis;
        let prepared = editor
            .prepare_source_tool_artifact_registration(&basis, Arc::clone(&installation))
            .unwrap();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        artifact_completed(editor, &attempt, ArtifactOutcome::Registered);
        quiet(editor);
        installation
    }

    fn graph_completed(
        editor: &mut EditorProjectSession,
        attempt: &EditorSourceToolAttemptId,
        expected: SelectionOutcome,
    ) {
        wait(editor, |editor| {
            let view = editor.source_tool_selection_view();
            view.phase == SelectionPhase::Idle
                && view.operation.as_ref().is_some_and(|operation| {
                    &operation.attempt == attempt && operation.outcome.is_some()
                })
        });
        assert_eq!(
            editor
                .source_tool_selection_view()
                .operation
                .unwrap()
                .outcome,
            Some(expected),
        );
        quiet(editor);
    }

    fn graph_roundtrip(
        editor: &mut EditorProjectSession,
        project: &Project,
        installation: &EditorSourceToolInstallation,
    ) {
        let artifact_basis = editor.source_tool_artifact_view().basis;
        let selection_basis = editor.source_tool_selection_view().basis;
        let prepared = editor
            .prepare_source_tool_installations(
                &selection_basis,
                writable_root(project),
                &[EditorSourceToolInstallationRequest {
                    directory: installation.project_path().into(),
                    receipt: installation.receipt_digest(),
                    requirements: Vec::new(),
                }],
            )
            .unwrap();
        let attempt = editor.register_source_tool_installations(prepared).unwrap();
        graph_completed(editor, &attempt, SelectionOutcome::Registered);
        assert!(matches!(
            editor.prepare_source_tool_reclamation(
                &artifact_basis,
                &editor.source_tool_selection_view().basis,
                writable_root(project),
                installation.receipt().installation_id(),
            ),
            Err(Error::StillInstalled)
        ));
        let prepared = editor
            .prepare_source_tool_withdrawal(
                &editor.source_tool_selection_view().basis,
                installation.receipt().installation_id(),
                EditorSourceToolWithdrawalMode::RejectDependents,
            )
            .unwrap();
        let attempt = editor.withdraw_source_tool_installations(prepared).unwrap();
        graph_completed(editor, &attempt, SelectionOutcome::Withdrawn);
        assert!(editor.source_tool_selection_view().installed.is_empty());
        assert!(matches!(
            editor.prepare_source_tool_reclamation(
                &artifact_basis,
                &selection_basis,
                writable_root(project),
                installation.receipt().installation_id(),
            ),
            Err(Error::StaleBasis)
        ));
    }

    fn prepare(
        editor: &mut EditorProjectSession,
        project: &Project,
        installation: &EditorSourceToolInstallation,
    ) -> PreparedEditorSourceToolReclamation {
        quiet(editor);
        editor
            .prepare_source_tool_reclamation(
                &editor.source_tool_artifact_view().basis,
                &editor.source_tool_selection_view().basis,
                writable_root(project),
                installation.receipt().installation_id(),
            )
            .unwrap()
    }

    fn gate(editor: &mut EditorProjectSession, after: usize) -> RemovalControl {
        let (entered, entrance) = mpsc::channel();
        let (release, released) = mpsc::channel();
        editor.source_tools.reclamation.next_gate = Some(Arc::new(RemovalGate {
            entered,
            release: Mutex::new(released),
            after,
        }));
        RemovalControl {
            entered: entrance,
            release,
        }
    }

    fn entered(editor: &mut EditorProjectSession, control: &RemovalControl, index: usize) {
        wait(editor, |editor| {
            editor.source_tool_reclamation_view().phase == Phase::Removing
        });
        assert_eq!(control.entered.recv_timeout(WAIT).unwrap(), index);
        let started = Instant::now();
        editor.advance_source_tool_reclamation();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "poll blocked on held removal work"
        );
        assert!(editor.source_tool_reclamation_view().physical_work_pending);
    }

    fn completed(editor: &mut EditorProjectSession, expected: Outcome) -> Record {
        wait(editor, |editor| {
            let view = editor.source_tool_reclamation_view();
            view.phase == Phase::Terminal && !view.physical_work_pending
        });
        let view = editor.source_tool_reclamation_view();
        assert_eq!(view.outcome, Some(expected));
        assert!(!view.record_publication_pending);
        view.record.unwrap()
    }

    fn disk(project: &Project) -> Vec<Record> {
        ArtifactState::test_decode_reclamations(
            &fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(),
        )
    }

    #[test]
    fn dropped_and_stale_reviews_do_not_delete_and_complete_removes_only_declared_members() {
        let project = Project::new();
        let mut saved = None;
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let directory = project.root.join(installation.project_path());
            fs::write(directory.join(UNKNOWN), b"not declared; preserve me").unwrap();
            let before = fs::read(project.root.join(ARTIFACTS_PATH)).unwrap();
            let dropped = prepare(editor, &project, &installation);
            assert!(dropped.creation_custody());
            drop(dropped);
            assert_eq!(fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(), before);
            for member in installation.members() {
                assert!(directory.join(member.path()).is_file());
            }
            let stale = prepare(editor, &project, &installation);
            let basis = editor.source_tool_artifact_view().basis;
            let observation = editor
                .prepare_source_tool_artifact_observation(
                    &basis,
                    installation.receipt().installation_id(),
                )
                .unwrap();
            let attempt = editor.commit_source_tool_artifacts(observation).unwrap();
            artifact_completed(editor, &attempt, ArtifactOutcome::Observed);
            assert_eq!(
                editor.confirm_source_tool_reclamation(stale),
                Err(Error::StaleBasis)
            );
            let prepared = prepare(editor, &project, &installation);
            let id = prepared.record().id;
            editor.confirm_source_tool_reclamation(prepared).unwrap();
            let record = completed(editor, Outcome::Complete);
            assert_eq!(record.id, id);
            assert_eq!(record.phase, RecordPhase::Results);
            assert!(
                record
                    .members
                    .iter()
                    .all(|member| member.state == MemberState::Removed)
            );
            for member in &record.members {
                assert!(!directory.join(member.path.as_ref()).exists());
            }
            assert_eq!(
                fs::read(directory.join(UNKNOWN)).unwrap(),
                b"not declared; preserve me"
            );
            assert_eq!(disk(&project), vec![record.clone()]);
            assert!(editor.source_tools.active.is_none());
            saved = Some((installation.project_path().to_owned(), record));
        });
        let (directory, record) = saved.unwrap();
        project.run(|editor| {
            quiet(editor);
            assert_eq!(
                editor.source_tool_artifact_view().reclamations,
                vec![record]
            );
            assert_eq!(editor.source_tool_reclamation_view().phase, Phase::Idle);
            assert!(editor.source_tool_reclamation_view().attempt.is_none());
            for _ in 0..3 {
                editor.drive_editor_frame(Duration::ZERO);
            }
            assert_eq!(
                fs::read(project.root.join(&directory).join(UNKNOWN)).unwrap(),
                b"not declared; preserve me"
            );
            assert!(editor.source_tools.active.is_none());
        });
    }

    #[test]
    fn failed_intent_publication_never_unlinks_a_member() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            let before = fs::read(project.root.join(ARTIFACTS_PATH)).unwrap();
            let control = editor.source_tools.artifacts.test_fail_next_publication();
            editor.confirm_source_tool_reclamation(prepared).unwrap();
            wait(editor, |editor| {
                editor.source_tool_artifact_view().error.is_some()
            });
            assert!(editor.source_tool_artifact_view().physical_work_pending);
            assert_eq!(fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(), before);
            for member in installation.members() {
                assert!(
                    project
                        .root
                        .join(installation.project_path())
                        .join(member.path())
                        .is_file()
                );
            }
            control.release();
            editor
                .source_tools
                .artifacts
                .test_allow_publication_cleanup();
            let record = completed(editor, Outcome::Failed(Error::Write));
            assert_eq!(record.phase, RecordPhase::Intent);
            assert!(
                record
                    .members
                    .iter()
                    .all(|member| member.state == MemberState::Pending)
            );
            assert!(disk(&project).is_empty());
        });
    }

    #[test]
    fn another_ready_identity_at_the_same_directory_still_blocks_reclamation() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let release = Release::new();
            let captured = PreparedEditorSourceToolRelease::capture(
                &release.root(TrustMode::Untrusted),
                release.write_manifest(),
            )
            .unwrap();
            assert_eq!(
                captured.manifest_digest(),
                installation.receipt().manifest_digest()
            );
            let replacement = match EditorSourceToolInstallationPlan::new(
                writable_root(&project),
                RelativeComponent::new("different-ready-identity").unwrap(),
                captured,
            )
            .unwrap()
            .publish()
            {
                EditorSourceToolInstallationOutcome::Published(value) => value,
                other => panic!("expected actual ready publication: {other:?}"),
            };
            assert_ne!(
                replacement.receipt().installation_id(),
                installation.receipt().installation_id()
            );
            fs::copy(
                project
                    .root
                    .join(replacement.project_path())
                    .join(".nara-installation.json"),
                project
                    .root
                    .join(installation.project_path())
                    .join(".nara-installation.json"),
            )
            .unwrap();
            let prepared = editor
                .prepare_source_tool_installations(
                    &editor.source_tool_selection_view().basis,
                    writable_root(&project),
                    &[EditorSourceToolInstallationRequest {
                        directory: installation.project_path().into(),
                        receipt: replacement.receipt_digest(),
                        requirements: Vec::new(),
                    }],
                )
                .unwrap();
            let attempt = editor.register_source_tool_installations(prepared).unwrap();
            graph_completed(editor, &attempt, SelectionOutcome::Registered);
            let selected = editor.source_tool_selection_view();
            assert_eq!(
                &selected.installed[0].installed.installation,
                replacement.receipt().installation_id()
            );
            assert!(matches!(
                editor.prepare_source_tool_reclamation(
                    &editor.source_tool_artifact_view().basis,
                    &selected.basis,
                    writable_root(&project),
                    installation.receipt().installation_id(),
                ),
                Err(Error::StillInstalled)
            ));
            assert!(disk(&project).is_empty());
            for member in installation.members() {
                assert!(
                    project
                        .root
                        .join(installation.project_path())
                        .join(member.path())
                        .is_file()
                );
            }
        });
    }

    #[test]
    fn unavailable_selection_never_supplies_graph_absence() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let selection = project.root.join(
                crate::project_host::runtime::editor::source_tools::selection::SELECTION_PATH,
            );
            let original = fs::read(&selection).unwrap();
            fs::write(&selection, b"invalid selection").unwrap();
            editor
                .reconcile_source_tool_selection(&editor.source_tool_selection_view().basis)
                .unwrap();
            wait(editor, |editor| {
                editor.source_tool_selection_view().phase == SelectionPhase::Unavailable
            });
            assert!(matches!(
                editor.prepare_source_tool_reclamation(
                    &editor.source_tool_artifact_view().basis,
                    &editor.source_tool_selection_view().basis,
                    writable_root(&project),
                    installation.receipt().installation_id(),
                ),
                Err(Error::RequiresReconcile)
            ));
            for member in installation.members() {
                assert!(
                    project
                        .root
                        .join(installation.project_path())
                        .join(member.path())
                        .is_file()
                );
            }
            assert!(disk(&project).is_empty());
            fs::write(selection, original).unwrap();
        });
    }

    #[test]
    fn selection_drift_after_persisted_intent_refuses_every_unlink() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            let reviewed = prepared.record().clone();
            editor.confirm_source_tool_reclamation(prepared).unwrap();
            let deadline = Instant::now() + WAIT;
            loop {
                editor.advance_source_tool_artifacts();
                let view = editor.source_tool_artifact_view();
                if !view.physical_work_pending
                    && view.operation.as_ref().is_some_and(|operation| {
                        operation.outcome == Some(ArtifactOutcome::ReclamationRecorded)
                    })
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "intent did not physically return"
                );
                std::thread::yield_now();
            }
            assert_eq!(disk(&project), vec![reviewed.clone()]);
            assert_eq!(
                editor.source_tool_reclamation_view().phase,
                Phase::PublishingIntent
            );
            let selection = project.root.join(
                crate::project_host::runtime::editor::source_tools::selection::SELECTION_PATH,
            );
            let original = fs::read(&selection).unwrap();
            let mut drifted = original.clone();
            drifted.push(b' ');
            fs::write(&selection, drifted).unwrap();
            let actual = completed(editor, Outcome::Partial);
            assert_eq!(
                editor.source_tool_reclamation_view().error,
                Some(Error::Changed)
            );
            assert_eq!(actual.members, reviewed.members);
            assert_eq!(actual.phase, RecordPhase::Results);
            for member in installation.members() {
                assert!(
                    project
                        .root
                        .join(installation.project_path())
                        .join(member.path())
                        .is_file()
                );
            }
            assert!(
                editor
                    .source_tools
                    .artifacts
                    .creation_custody(installation.receipt().installation_id(),)
                    .is_some()
            );
            fs::write(selection, original).unwrap();
        });
    }

    #[test]
    fn cancellation_after_one_actual_unlink_preserves_later_members_and_records_the_removal() {
        let project = Project::new();
        let mut saved = None;
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            assert_eq!(prepared.record().members[0].path.as_ref(), ".nara-installation.json");
            let control = gate(editor, 0);
            let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
            entered(editor, &control, 0);
            let directory = project.root.join(installation.project_path());
            assert!(!directory.join(".nara-installation.json").exists());
            editor.cancel_source_tool_reclamation(&attempt).unwrap();
            let started = Instant::now();
            editor.advance_source_tool_reclamation();
            assert!(started.elapsed() < Duration::from_secs(2));
            drop(control);
            let record = completed(editor, Outcome::Cancelled);
            assert_eq!(record.members[0].state, MemberState::Removed);
            for member in &record.members[1..] {
                assert_eq!(member.state, MemberState::Pending);
                assert!(directory.join(member.path.as_ref()).is_file());
            }
            assert_eq!(disk(&project), vec![record.clone()]);
            let original = directory.join("samples/readme.txt");
            let renamed = project.root.join("renamed-remainder.txt");
            fs::rename(&original, &renamed).unwrap();
            let moved = writable_root(&project).open_file_exact(
                &nara_fs::RelativePath::new("renamed-remainder.txt").unwrap(),
            ).unwrap();
            assert_eq!(editor.exclude_source_aliases("renamed-remainder.txt", &moved),
                Err(crate::project_host::runtime::editor::source_admission::SourceAdmissionError::PrivateArtifact));
            fs::rename(renamed, original).unwrap();
            saved = Some((installation.project_path().to_owned(), record));
        });
        let (directory, record) = saved.unwrap();
        project.run(|editor| {
            quiet(editor);
            assert_eq!(
                editor.source_tool_artifact_view().reclamations,
                vec![record.clone()]
            );
            assert_eq!(editor.source_tool_reclamation_view().phase, Phase::Idle);
            for _ in 0..3 {
                editor.drive_editor_frame(Duration::ZERO);
            }
            for member in &record.members[1..] {
                assert!(
                    project
                        .root
                        .join(&directory)
                        .join(member.path.as_ref())
                        .is_file()
                );
            }
            assert!(editor.source_tools.active.is_none());
        });
    }

    #[test]
    fn close_revokes_new_work_before_held_removal_returns_and_preserves_completed_unlink() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            let control = gate(editor, 0);
            let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
            entered(editor, &control, 0);
            assert!(matches!(
                editor.advance_close(),
                EditorProjectCloseProgress::Incomplete { .. }
            ));
            assert!(!editor.accepts_new_work());
            assert_eq!(
                editor.retry_source_tool_reclamation_record(
                    &attempt,
                    &editor.source_tool_artifact_view().basis,
                ),
                Err(Error::Closing)
            );
            drop(control);
            let deadline = Instant::now() + WAIT;
            let receipt = loop {
                match editor.advance_close() {
                    EditorProjectCloseProgress::Complete(receipt) => break receipt,
                    EditorProjectCloseProgress::Incomplete { .. } => {
                        assert!(
                            Instant::now() < deadline,
                            "Close did not settle owned results"
                        );
                        std::thread::yield_now();
                    }
                }
            };
            assert!(!receipt.has_cleanup_failure());
            let view = editor.source_tool_reclamation_view();
            assert_eq!(view.outcome, Some(Outcome::Cancelled));
            assert!(!view.physical_work_pending && !view.record_publication_pending);
            let actual = view.record.unwrap();
            assert_eq!(actual.members[0].state, MemberState::Removed);
            assert!(
                actual.members[1..]
                    .iter()
                    .all(|member| member.state == MemberState::Pending)
            );
            assert_eq!(disk(&project), vec![actual]);
        });
    }

    #[test]
    fn failed_results_publication_retains_actual_results_until_explicit_reconcile_and_retry() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            let control = gate(editor, 0);
            let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
            entered(editor, &control, 0);
            let failure = editor.source_tools.artifacts.test_fail_next_publication();
            drop(control);
            wait(editor, |editor| {
                editor.source_tool_artifact_view().error.is_some()
            });
            assert!(editor.source_tool_artifact_view().physical_work_pending);
            failure.release();
            editor
                .source_tools
                .artifacts
                .test_allow_publication_cleanup();
            wait(editor, |editor| {
                let view = editor.source_tool_reclamation_view();
                view.phase == Phase::Terminal && !view.physical_work_pending
            });
            let failed = editor.source_tool_reclamation_view();
            assert_eq!(failed.outcome, Some(Outcome::Failed(Error::Write)));
            assert!(failed.record_publication_pending);
            let actual = failed.record.unwrap();
            assert_eq!(actual.phase, RecordPhase::Results);
            assert!(
                actual
                    .members
                    .iter()
                    .all(|member| member.state == MemberState::Removed)
            );
            let persisted = disk(&project);
            assert_eq!(persisted[0].id, actual.id);
            assert_eq!(persisted[0].phase, RecordPhase::Intent);
            assert!(
                persisted[0]
                    .members
                    .iter()
                    .all(|member| member.state == MemberState::Pending)
            );
            for member in &actual.members {
                assert!(
                    !project
                        .root
                        .join(installation.project_path())
                        .join(member.path.as_ref())
                        .exists()
                );
            }
            let basis = editor.source_tool_artifact_view().basis;
            let reconcile = editor.reconcile_source_tool_artifacts(&basis).unwrap();
            let reconciled = artifact_completed(editor, &reconcile, ArtifactOutcome::Reconciled);
            assert_eq!(
                editor.source_tool_reclamation_view().record.as_ref(),
                Some(&actual)
            );
            editor
                .retry_source_tool_reclamation_record(&attempt, &reconciled.basis)
                .unwrap();
            assert_eq!(completed(editor, Outcome::Complete), actual);
            assert_eq!(disk(&project), vec![actual]);
        });
    }

    #[test]
    fn equal_bytes_file_replacement_invalidates_review_before_intent() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            let directory = project.root.join(installation.project_path());
            let original = directory.join("samples/readme.txt");
            let renamed = directory.join("samples/original-readme.txt");
            let bytes = fs::read(&original).unwrap();
            let inventory = fs::read(project.root.join(ARTIFACTS_PATH)).unwrap();
            fs::rename(&original, &renamed).unwrap();
            fs::write(&original, &bytes).unwrap();
            assert_eq!(
                editor.confirm_source_tool_reclamation(prepared),
                Err(Error::Changed)
            );
            assert_eq!(fs::read(original).unwrap(), bytes);
            assert_eq!(fs::read(renamed).unwrap(), bytes);
            assert_eq!(
                fs::read(project.root.join(ARTIFACTS_PATH)).unwrap(),
                inventory
            );
            assert!(disk(&project).is_empty());
            assert_eq!(editor.source_tool_reclamation_view().phase, Phase::Idle);
        });
    }

    #[test]
    fn prepared_review_excludes_directory_delete_access_until_dropped() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let prepared = prepare(editor, &project, &installation);
            let directory = project.root.join(installation.project_path());
            let delete_handle = || {
                fs::OpenOptions::new()
                    .read(true)
                    .access_mode(0x0001_0000)
                    .share_mode(7)
                    .custom_flags(0x0200_0000 | 0x0020_0000)
                    .open(&directory)
            };
            assert!(delete_handle().is_err());
            assert!(directory.is_dir());
            drop(prepared);
            drop(delete_handle().unwrap());
            assert!(disk(&project).is_empty());
        });
    }

    #[test]
    fn locked_later_member_reports_partial_and_preserves_every_future_member() {
        let project = Project::new();
        project.run(|editor| {
            let installation = register(editor, &project);
            graph_roundtrip(editor, &project, &installation);
            let directory = project.root.join(installation.project_path());
            // Allow preparation's reads and existing writers, but deny delete access.
            let locked = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0x0000_0001 | 0x0000_0002)
                .open(directory.join("Tool.dll"))
                .unwrap();
            let prepared = prepare(editor, &project, &installation);
            let reviewed = prepared.record().clone();
            assert_eq!(reviewed.members[0].path.as_ref(), ".nara-installation.json");
            assert_eq!(reviewed.members[1].path.as_ref(), "Tool.dll");
            assert!(reviewed.members.len() > 2);
            let original: Vec<_> = reviewed
                .members
                .iter()
                .map(|member| fs::read(directory.join(member.path.as_ref())).unwrap())
                .collect();
            editor.confirm_source_tool_reclamation(prepared).unwrap();
            let actual = completed(editor, Outcome::Partial);
            assert_eq!(actual.id, reviewed.id);
            assert_eq!(actual.installation, reviewed.installation);
            assert_eq!(actual.phase, RecordPhase::Results);
            assert_eq!(actual.members.len(), reviewed.members.len());
            assert_eq!(
                editor.source_tool_reclamation_view().error,
                Some(Error::Write)
            );
            for (index, member) in actual.members.iter().enumerate() {
                assert_eq!(member.path, reviewed.members[index].path);
                let expected = match index {
                    0 => MemberState::Removed,
                    1 => MemberState::Failed,
                    _ => MemberState::Pending,
                };
                assert_eq!(member.state, expected, "{}", member.path);
                let path = directory.join(member.path.as_ref());
                if index == 0 {
                    assert!(!path.exists());
                } else {
                    assert_eq!(fs::read(path).unwrap(), original[index]);
                }
            }
            assert_eq!(disk(&project), vec![actual]);
            drop(locked);
        });
    }

    #[test]
    fn failed_results_publication_closes_physically_with_exact_unpublished_mixed_results() {
        let project = Project::new();
        // Project::run requires a clean Close receipt; this failure is intentionally retained.
        let mut editor = project.open();
        let installation = register(&mut editor, &project);
        graph_roundtrip(&mut editor, &project, &installation);
        let directory = project.root.join(installation.project_path());
        let locked = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0x0000_0001 | 0x0000_0002)
            .open(directory.join("Tool.dll"))
            .unwrap();
        let prepared = prepare(&mut editor, &project, &installation);
        let reviewed = prepared.record().clone();
        assert_eq!(reviewed.members[0].path.as_ref(), ".nara-installation.json");
        assert_eq!(reviewed.members[1].path.as_ref(), "Tool.dll");
        assert!(reviewed.members.len() > 2);
        let original: Vec<_> = reviewed
            .members
            .iter()
            .map(|member| fs::read(directory.join(member.path.as_ref())).unwrap())
            .collect();
        let control = gate(&mut editor, 1);
        let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
        entered(&mut editor, &control, 1);
        assert!(!directory.join(reviewed.members[0].path.as_ref()).exists());
        assert_eq!(fs::read(directory.join("Tool.dll")).unwrap(), original[1]);
        let intent = disk(&project)[0].clone();
        assert_eq!(intent, reviewed);
        let failure = editor.source_tools.artifacts.test_fail_next_publication();
        drop(control);
        wait(&mut editor, |editor| {
            editor.source_tool_artifact_view().error.is_some()
        });
        assert!(editor.source_tool_artifact_view().physical_work_pending);
        failure.release();
        editor
            .source_tools
            .artifacts
            .test_allow_publication_cleanup();
        wait(&mut editor, |editor| {
            let view = editor.source_tool_reclamation_view();
            view.phase == Phase::Terminal && !view.physical_work_pending
        });
        let failed = editor.source_tool_reclamation_view();
        assert_eq!(failed.attempt.as_ref(), Some(&attempt));
        assert_eq!(failed.outcome, Some(Outcome::Failed(Error::Write)));
        assert_eq!(failed.error, Some(Error::Write));
        assert!(failed.record_publication_pending);
        let actual = failed.record.as_ref().unwrap();
        assert_eq!(actual.id, reviewed.id);
        assert_eq!(actual.installation, reviewed.installation);
        assert_eq!(actual.phase, RecordPhase::Results);
        assert_eq!(actual.members.len(), reviewed.members.len());
        for (index, member) in actual.members.iter().enumerate() {
            assert_eq!(member.path, reviewed.members[index].path);
            let expected = match index {
                0 => MemberState::Removed,
                1 => MemberState::Failed,
                _ => MemberState::Pending,
            };
            assert_eq!(member.state, expected, "{}", member.path);
        }
        assert_eq!(disk(&project), vec![intent.clone()]);
        let deadline = Instant::now() + WAIT;
        let receipt = loop {
            let started = Instant::now();
            let progress = editor.advance_close();
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "Close poll blocked while retaining unpublished reclamation results",
            );
            match progress {
                EditorProjectCloseProgress::Complete(receipt) => break receipt,
                progress => assert!(
                    Instant::now() < deadline,
                    "unpublished reclamation must not prevent physical Close: {progress:?}",
                ),
            }
            thread::sleep(Duration::from_millis(1));
        };
        assert!(receipt.task_shutdown().is_complete());
        assert!(receipt.has_cleanup_failure());
        assert_eq!(receipt.unpublished_reclamation(), Some(&failed));
        drop(editor);
        // Failure evidence survives the Session itself and does not rewrite durable Intent.
        assert_eq!(receipt.unpublished_reclamation(), Some(&failed));
        assert_eq!(disk(&project), vec![intent]);
        for (index, member) in actual.members.iter().enumerate() {
            let path = directory.join(member.path.as_ref());
            if index == 0 {
                assert!(!path.exists());
            } else {
                assert_eq!(fs::read(path).unwrap(), original[index]);
            }
        }
        drop(locked);
    }
}
