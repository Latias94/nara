//! Actual Session protection management; these tests do not qualify installed-file deletion.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(15);

/// The production IO worker calls this after IO, before returning its actual Task.
pub(super) struct IoGate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl IoGate {
    fn new() -> (Arc<Self>, IoControl) {
        let (entered, entrance) = mpsc::channel();
        let (release, released) = mpsc::channel();
        (
            Arc::new(Self {
                entered,
                release: Mutex::new(released),
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

struct IoControl {
    entrance: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

impl IoControl {
    fn wait_until_entered(&self) {
        self.entrance
            .recv_timeout(WAIT)
            .expect("the actual content IO Task must reach its return gate");
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
mod native {
    use std::{
        collections::BTreeMap,
        fs,
        sync::Arc,
        time::{Duration, Instant},
    };

    use nara_fs::{ContentDigest, RelativeComponent, TrustMode};
    use nara_scene::{
        SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
        SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
        ScenePlacement, ScenePlacementOwner,
    };
    use nara_tasks::TaskReturnState;
    use nara_tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorSourceToolAttemptId, EditorSourceToolContentAttempt,
        EditorSourceToolContentError as Error, EditorSourceToolContentInstallation,
        EditorSourceToolContentMember, EditorSourceToolContentObservation as Observation,
        EditorSourceToolContentOutcome as Outcome, EditorSourceToolContentPhase as Phase,
        EditorSourceToolContentRole as Role, EditorSourceToolContentView,
        EditorSourceToolSelectionPhase,
    };
    use serde_json::{Value, json};

    use super::super::PreparedEditorSourceToolContent;
    use super::IoGate;
    use crate::project_host::{
        EditorProjectCloseProgress, EditorProjectSession,
        runtime::editor::{
            source_admission::SourceAdmissionError,
            source_tools::{
                EditorSourceToolInstallation, EditorSourceToolInstallationOutcome,
                EditorSourceToolInstallationPlan, PreparedEditorSourceToolRelease,
                release::tests::Release,
                tests::fixture::{Project, wait},
            },
        },
    };

    const RECORD: &str = ".nara/source-tools/content.json";
    const DIRECTORY: &str = "installed-stats";
    const SAMPLE: &str = "samples/readme.txt";
    const UNKNOWN: &str = "installed-stats/samples/project-owned.txt";

    fn loaded(editor: &mut EditorProjectSession) -> EditorSourceToolContentView {
        wait(editor, |editor| {
            let content = editor.source_tool_content_view();
            let selection = editor.source_tool_selection_view();
            matches!(content.phase, Phase::Idle | Phase::Unavailable)
                && !content.physical_work_pending
                && content
                    .operation
                    .as_ref()
                    .is_some_and(|operation| operation.outcome.is_some())
                && matches!(
                    selection.phase,
                    EditorSourceToolSelectionPhase::Idle
                        | EditorSourceToolSelectionPhase::Unavailable
                )
        });
        editor.source_tool_content_view()
    }

    fn install(editor: &EditorProjectSession) -> Arc<EditorSourceToolInstallation> {
        let release = Release::new();
        let manifest = release.write_manifest();
        let captured =
            PreparedEditorSourceToolRelease::capture(&release.root(TrustMode::Untrusted), manifest)
                .unwrap();
        let plan = EditorSourceToolInstallationPlan::new(
            editor.source_project_root(),
            RelativeComponent::new(DIRECTORY).unwrap(),
            captured,
        )
        .unwrap();
        match plan.publish() {
            EditorSourceToolInstallationOutcome::Published(installation) => {
                installation.verify().unwrap();
                Arc::new(installation)
            }
            other => panic!("expected actual project installation publication: {other:?}"),
        }
    }

    fn completed(
        editor: &mut EditorProjectSession,
        attempt: &EditorSourceToolAttemptId,
        outcome: Outcome,
    ) -> EditorSourceToolContentView {
        wait(editor, |editor| {
            let view = editor.source_tool_content_view();
            !view.physical_work_pending
                && view.operation.as_ref().is_some_and(|operation| {
                    &operation.attempt == attempt && operation.outcome.is_some()
                })
        });
        let view = editor.source_tool_content_view();
        match &outcome {
            Outcome::Failed(error) => assert_eq!(view.error.as_ref(), Some(error)),
            Outcome::NeedsProtectionPublication => {
                assert_eq!(view.phase, Phase::RequiresReconcile, "{view:?}");
                assert!(view.protection_publication_pending, "{view:?}");
                assert!(view.error.is_none(), "{view:?}");
            }
            _ => {
                assert_eq!(view.phase, Phase::Idle, "{view:?}");
                assert!(!view.protection_publication_pending, "{view:?}");
                assert!(view.error.is_none(), "{view:?}");
            }
        }
        assert_eq!(
            view.operation,
            Some(EditorSourceToolContentAttempt {
                attempt: attempt.clone(),
                outcome: Some(outcome),
            }),
        );
        view
    }

    fn commit(
        editor: &mut EditorProjectSession,
        prepared: PreparedEditorSourceToolContent,
        outcome: Outcome,
    ) -> EditorSourceToolContentView {
        let basis = editor.source_tool_content_view().basis;
        let attempt = editor.commit_source_tool_content(prepared).unwrap();
        let publishing = editor.source_tool_content_view();
        assert_eq!(publishing.phase, Phase::Publishing);
        assert!(publishing.physical_work_pending);
        assert_ne!(publishing.basis, basis);
        assert_eq!(
            publishing.operation,
            Some(EditorSourceToolContentAttempt {
                attempt: attempt.clone(),
                outcome: None,
            }),
        );
        completed(editor, &attempt, outcome)
    }

    fn manage(
        editor: &mut EditorProjectSession,
        installation: &Arc<EditorSourceToolInstallation>,
    ) -> EditorSourceToolContentView {
        let basis = editor.source_tool_content_view().basis;
        let candidate = editor
            .prepare_source_tool_content_management(&basis, DIRECTORY, Arc::clone(installation))
            .unwrap();
        commit(editor, candidate, Outcome::Managed)
    }

    fn sample(view: &EditorSourceToolContentView) -> &EditorSourceToolContentMember {
        assert_eq!(view.installations.len(), 1);
        view.installations[0]
            .members
            .iter()
            .find(|member| member.path.as_ref() == SAMPLE)
            .unwrap()
    }

    fn assert_facts(
        entry: &EditorSourceToolContentInstallation,
        installation: &EditorSourceToolInstallation,
    ) {
        assert_eq!(entry.installed.package.as_ref(), installation.package());
        assert_eq!(entry.installed.directory.as_ref(), DIRECTORY);
        assert_eq!(
            &entry.installed.installation,
            installation.receipt().installation_id(),
        );
        assert_eq!(entry.installed.receipt, installation.receipt_digest());
        assert_eq!(
            entry.installed.manifest,
            installation.receipt().manifest_digest(),
        );
        assert_eq!(entry.members.len(), 5);
        for (path, role) in [
            ("nara-tool.json", Role::Metadata),
            (".nara-installation.json", Role::Metadata),
            ("catalog.json", Role::Tool),
            ("Tool.dll", Role::Tool),
            (SAMPLE, Role::Content),
        ] {
            let member = entry
                .members
                .iter()
                .find(|member| member.path.as_ref() == path)
                .unwrap();
            let created = installation
                .members()
                .iter()
                .find(|member| member.path() == path)
                .unwrap();
            assert_eq!(member.role, role, "{path}");
            assert_eq!(member.baseline, created.digest(), "{path}");
            assert_eq!(member.observation, Observation::Present, "{path}");
        }
    }

    fn assert_record(project: &Project, view: &EditorSourceToolContentView) {
        let entries: Vec<_> = view
            .installations
            .iter()
            .map(|entry| {
                let members: Vec<_> = entry
                    .members
                    .iter()
                    .map(|member| {
                        let role = match member.role {
                            Role::Tool => "tool",
                            Role::Content => "content",
                            Role::Metadata => "metadata",
                        };
                        let observation = match member.observation {
                            Observation::Present => "present",
                            Observation::Pending => "pending",
                            Observation::Removed => "removed",
                            Observation::Missing => "missing",
                            Observation::Failed => "failed",
                        };
                        json!({
                            "path": member.path.as_ref(),
                            "role": role,
                            "baseline": {
                                "length": member.baseline.length(),
                                "hash": member.baseline.as_bytes(),
                            },
                            "modified": member.modified,
                            "adopted": member.adopted,
                            "observation": observation,
                        })
                    })
                    .collect();
                json!({
                    "package": entry.installed.package.as_ref(),
                    "directory": entry.installed.directory.as_ref(),
                    "installation": entry.installed.installation,
                    "receipt": {
                        "length": entry.installed.receipt.length(),
                        "hash": entry.installed.receipt.as_bytes(),
                    },
                    "manifest": {
                        "length": entry.installed.manifest.length(),
                        "hash": entry.installed.manifest.as_bytes(),
                    },
                    "members": members,
                })
            })
            .collect();
        let disk: Value =
            serde_json::from_slice(&fs::read(project.root.join(RECORD)).unwrap()).unwrap();
        assert_eq!(disk, json!({"format_version": 1, "installations": entries}));
    }

    fn file_snapshot(
        project: &Project,
        installation: &EditorSourceToolInstallation,
    ) -> BTreeMap<String, Vec<u8>> {
        installation
            .members()
            .iter()
            .map(|member| {
                let path = format!("{DIRECTORY}/{}", member.path());
                let bytes = fs::read(project.root.join(&path)).unwrap();
                (path, bytes)
            })
            .collect()
    }

    fn assert_files(project: &Project, expected: &BTreeMap<String, Vec<u8>>) {
        for (path, bytes) in expected {
            assert_eq!(&fs::read(project.root.join(path)).unwrap(), bytes, "{path}");
        }
    }

    #[derive(Debug, PartialEq)]
    struct SourceSnapshot {
        source: SceneDocument,
        revision: SceneAuthoringRevision,
        history: SceneAuthoringHistoryStatus,
        usage: SceneAuthoringHistoryUsage,
        dirty: bool,
    }

    fn source_snapshot(editor: &EditorProjectSession) -> SourceSnapshot {
        let document = editor.workspace.active_document().unwrap();
        let slot = editor.workspace.scene(document).unwrap();
        SourceSnapshot {
            source: slot.session().document().clone(),
            revision: slot.revision(),
            history: slot.session().history_status(),
            usage: slot.session().history_usage(),
            dirty: slot.is_dirty(),
        }
    }

    fn assert_no_execution(editor: &EditorProjectSession) {
        let tool = editor.source_tool_view();
        assert!(tool.active.is_none(), "{tool:?}");
        assert!(tool.attempt.is_none(), "{tool:?}");
        assert!(tool.review.is_none(), "{tool:?}");
        assert!(!tool.probing, "{tool:?}");
        assert!(!tool.retiring, "{tool:?}");
        assert!(editor.source_tool_selection_view().active_origin.is_none());
    }

    #[test]
    fn published_management_and_default_equal_adoption_survive_actual_close_and_fresh_open() {
        let project = Project::new();
        let original_source = project.persisted();
        let mut expected = Vec::new();
        let mut files = BTreeMap::new();
        let mut record = Vec::new();
        project.run(|editor| {
            let before = loaded(editor);
            assert_eq!(before.phase, Phase::Idle);
            assert!(before.installations.is_empty());
            let source = source_snapshot(editor);
            let selection = editor.source_tool_selection_view();
            let installation = install(editor);
            files = file_snapshot(&project, &installation);
            fs::write(project.root.join(UNKNOWN), b"project-owned sibling").unwrap();
            let candidate = editor
                .prepare_source_tool_content_management(
                    &before.basis,
                    DIRECTORY,
                    Arc::clone(&installation),
                )
                .unwrap();
            let preview = candidate.installations();
            assert_eq!(preview.len(), 1);
            assert_facts(&preview[0], &installation);
            assert!(preview[0].creation_custody);
            assert!(
                preview[0]
                    .members
                    .iter()
                    .all(|member| { !member.modified && !member.adopted })
            );
            assert_eq!(editor.source_tool_content_view(), before);
            assert!(!project.root.join(RECORD).exists());
            drop(candidate);
            assert!(!project.root.join(RECORD).exists());
            assert_eq!(editor.source_tool_content_view(), before);
            assert_files(&project, &files);

            let managed = manage(editor, &installation);
            assert_facts(&managed.installations[0], &installation);
            assert!(managed.installations[0].creation_custody);
            assert_record(&project, &managed);
            let managed_bytes = fs::read(project.root.join(RECORD)).unwrap();
            let id = *installation.receipt().installation_id();
            let adoption = editor
                .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                .unwrap();
            assert!(adoption.installations()[0].members.iter().any(|member| {
                member.path.as_ref() == SAMPLE && member.adopted && !member.modified
            }));
            assert_eq!(editor.source_tool_content_view(), managed);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), managed_bytes);
            drop(adoption);
            assert_eq!(editor.source_tool_content_view(), managed);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), managed_bytes);
            let adoption = editor
                .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                .unwrap();
            let adopted = commit(editor, adoption, Outcome::Adopted);
            assert!(sample(&adopted).adopted);
            assert!(!sample(&adopted).modified);
            assert!(adopted.installations[0].creation_custody);
            assert_record(&project, &adopted);
            assert_files(&project, &files);
            assert_eq!(
                fs::read(project.root.join(UNKNOWN)).unwrap(),
                b"project-owned sibling",
            );
            assert_eq!(source_snapshot(editor), source);
            assert_eq!(project.persisted(), original_source);
            assert_eq!(editor.source_tool_selection_view(), selection);
            assert_no_execution(editor);
            expected = adopted.installations;
            expected[0].creation_custody = false;
            record = fs::read(project.root.join(RECORD)).unwrap();
        });
        assert_files(&project, &files);
        project.run(|editor| {
            let reopened = loaded(editor);
            assert_eq!(reopened.installations, expected);
            assert_eq!(
                reopened.operation.as_ref().unwrap().outcome,
                Some(Outcome::Observed),
            );
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
            assert_eq!(project.persisted(), original_source);
            assert_eq!(
                fs::read(project.root.join(UNKNOWN)).unwrap(),
                b"project-owned sibling",
            );
            assert!(project.root.join(format!("{DIRECTORY}/samples")).is_dir());
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn observed_modification_and_adoption_are_sticky_and_stale_candidates_cannot_publish() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let source = source_snapshot(editor);
            let installation = install(editor);
            let id = *installation.receipt().installation_id();
            let managed = manage(editor, &installation);
            let sample_path = project.root.join(DIRECTORY).join(SAMPLE);
            let baseline = fs::read(&sample_path).unwrap();
            let unknown = project.root.join(DIRECTORY).join("samples/unknown.txt");
            fs::write(&unknown, b"unmanaged project content").unwrap();
            fs::write(&sample_path, b"project-authored sample").unwrap();
            let before_observation = fs::read(project.root.join(RECORD)).unwrap();
            let candidate = editor
                .prepare_source_tool_content_observation(&managed.basis, &id)
                .unwrap();
            assert!(
                candidate.installations()[0]
                    .members
                    .iter()
                    .any(|member| { member.path.as_ref() == SAMPLE && member.modified })
            );
            assert_eq!(editor.source_tool_content_view(), managed);
            assert_eq!(
                fs::read(project.root.join(RECORD)).unwrap(),
                before_observation,
            );
            let modified = commit(editor, candidate, Outcome::Observed);
            assert!(sample(&modified).modified);
            assert_eq!(
                sample(&modified).baseline,
                ContentDigest::of_bytes(&baseline)
            );
            fs::write(&sample_path, &baseline).unwrap();
            let candidate = editor
                .prepare_source_tool_content_observation(&modified.basis, &id)
                .unwrap();
            let restored = commit(editor, candidate, Outcome::Observed);
            assert!(sample(&restored).modified);
            let repeated = manage(editor, &installation);
            assert!(sample(&repeated).modified);
            assert!(!sample(&repeated).adopted);

            let before_refusal = fs::read(project.root.join(RECORD)).unwrap();
            for path in [
                "Tool.dll",
                "catalog.json",
                "nara-tool.json",
                ".nara-installation.json",
            ] {
                assert!(matches!(
                    editor.prepare_source_tool_content_adoption(&repeated.basis, &id, &[path]),
                    Err(Error::InvalidPath),
                ));
            }
            for paths in [
                vec!["samples/unknown.txt"],
                vec![SAMPLE, "samples/unknown.txt"],
            ] {
                assert!(matches!(
                    editor.prepare_source_tool_content_adoption(&repeated.basis, &id, &paths),
                    Err(Error::Unmanaged),
                ));
            }
            assert_eq!(editor.source_tool_content_view(), repeated);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), before_refusal);
            let stale_manage = editor
                .prepare_source_tool_content_management(
                    &repeated.basis,
                    DIRECTORY,
                    Arc::clone(&installation),
                )
                .unwrap();
            let stale_observation = editor
                .prepare_source_tool_content_observation(&repeated.basis, &id)
                .unwrap();
            let adoption = editor
                .prepare_source_tool_content_adoption(&repeated.basis, &id, &[SAMPLE])
                .unwrap();
            let adopted = commit(editor, adoption, Outcome::Adopted);
            let adopted_bytes = fs::read(project.root.join(RECORD)).unwrap();
            for candidate in [stale_manage, stale_observation] {
                assert_eq!(
                    editor.commit_source_tool_content(candidate),
                    Err(Error::StaleBasis),
                );
                assert_eq!(editor.source_tool_content_view(), adopted);
                assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), adopted_bytes);
            }
            let repeated = manage(editor, &installation);
            assert!(sample(&repeated).modified);
            assert!(sample(&repeated).adopted);
            assert_eq!(sample(&repeated).observation, Observation::Present);
            assert_record(&project, &repeated);
            assert_eq!(fs::read(&sample_path).unwrap(), baseline);
            assert_eq!(fs::read(&unknown).unwrap(), b"unmanaged project content");
            assert_eq!(source_snapshot(editor), source);
            assert_no_execution(editor);
        });
        project.run(|editor| {
            let reopened = loaded(editor);
            assert!(sample(&reopened).modified);
            assert!(sample(&reopened).adopted);
            assert!(!reopened.installations[0].creation_custody);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn equal_byte_member_replacement_is_observed_as_modified_and_stays_sticky() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let installation = install(editor);
            let files = file_snapshot(&project, &installation);
            let id = *installation.receipt().installation_id();
            let managed = manage(editor, &installation);
            let sample_path = project.root.join(DIRECTORY).join(SAMPLE);
            let original_path = sample_path.with_file_name("readme.original.txt");
            let baseline = fs::read(&sample_path).unwrap();
            fs::rename(&sample_path, &original_path).unwrap();
            fs::write(&sample_path, &baseline).unwrap();
            let candidate = editor
                .prepare_source_tool_content_observation(&managed.basis, &id)
                .unwrap();
            assert!(candidate.installations().iter().any(|installation| {
                installation
                    .members
                    .iter()
                    .any(|member| member.path.as_ref() == SAMPLE && member.modified)
            }));
            let observed = commit(editor, candidate, Outcome::Observed);
            assert!(sample(&observed).modified);
            assert_eq!(fs::read(&sample_path).unwrap(), baseline);
            fs::rename(
                &sample_path,
                sample_path.with_file_name("readme.replacement.txt"),
            )
            .unwrap();
            fs::rename(&original_path, &sample_path).unwrap();
            let candidate = editor
                .prepare_source_tool_content_observation(&observed.basis, &id)
                .unwrap();
            let restored = commit(editor, candidate, Outcome::Observed);
            assert!(sample(&restored).modified);
            assert_eq!(fs::read(&sample_path).unwrap(), baseline);
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn external_ledger_edits_and_namespace_replacement_require_explicit_reconcile() {
        for replace_leaf in [false, true] {
            let project = Project::new();
            project.run(|editor| {
                loaded(editor);
                let source = source_snapshot(editor);
                let installation = install(editor);
                let files = file_snapshot(&project, &installation);
                let id = *installation.receipt().installation_id();
                let managed = manage(editor, &installation);
                let candidate = editor
                    .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                    .unwrap();
                let path = project.root.join(RECORD);
                let original = fs::read(&path).unwrap();
                let external = if replace_leaf {
                    fs::rename(&path, path.with_file_name("external-content.json")).unwrap();
                    original.clone()
                } else {
                    let mut wire: Value = serde_json::from_slice(&original).unwrap();
                    for member in wire["installations"][0]["members"].as_array_mut().unwrap() {
                        if member["path"] == SAMPLE {
                            member["adopted"] = json!(true);
                        }
                    }
                    serde_json::to_vec(&wire).unwrap()
                };
                fs::write(&path, &external).unwrap();
                editor.drive_editor_frame(Duration::ZERO);
                assert_eq!(editor.source_tool_content_view(), managed);
                let refused = commit(editor, candidate, Outcome::Failed(Error::Changed));
                assert_eq!(refused.installations, managed.installations);
                assert_eq!(fs::read(&path).unwrap(), external);
                editor.drive_editor_frame(Duration::ZERO);
                assert_eq!(editor.source_tool_content_view(), refused);
                let attempt = editor
                    .reconcile_source_tool_content(&refused.basis)
                    .unwrap();
                let reconciled = completed(editor, &attempt, Outcome::Reconciled);
                assert_eq!(sample(&reconciled).adopted, !replace_leaf);
                assert!(reconciled.installations[0].creation_custody);
                assert_eq!(fs::read(&path).unwrap(), external);
                let adoption = editor
                    .prepare_source_tool_content_adoption(&reconciled.basis, &id, &[SAMPLE])
                    .unwrap();
                let adopted = commit(editor, adoption, Outcome::Adopted);
                assert!(sample(&adopted).adopted);
                assert_record(&project, &adopted);
                if replace_leaf {
                    assert_eq!(
                        fs::read(path.with_file_name("external-content.json")).unwrap(),
                        original,
                    );
                }
                assert_files(&project, &files);
                assert_eq!(source_snapshot(editor), source);
                assert_no_execution(editor);
            });
        }
    }

    #[test]
    fn explicit_reconcile_repair_preserves_rolled_back_protection_across_immediate_close() {
        let project = Project::new();
        let mut expected = Vec::new();
        let mut files = BTreeMap::new();
        project.run(|editor| {
            loaded(editor);
            let source = source_snapshot(editor);
            let installation = install(editor);
            let id = *installation.receipt().installation_id();
            let managed = manage(editor, &installation);
            fs::write(
                project.root.join(DIRECTORY).join(SAMPLE),
                b"observed project-owned sample",
            )
            .unwrap();
            let observation = editor
                .prepare_source_tool_content_observation(&managed.basis, &id)
                .unwrap();
            let modified = commit(editor, observation, Outcome::Observed);
            let adoption = editor
                .prepare_source_tool_content_adoption(&modified.basis, &id, &[SAMPLE])
                .unwrap();
            let protected = commit(editor, adoption, Outcome::Adopted);
            files = file_snapshot(&project, &installation);
            let path = project.root.join(RECORD);
            let mut rollback: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            for member in rollback["installations"][0]["members"]
                .as_array_mut()
                .unwrap()
            {
                if member["path"] == SAMPLE {
                    member["modified"] = json!(false);
                    member["adopted"] = json!(false);
                }
            }
            fs::write(&path, serde_json::to_vec(&rollback).unwrap()).unwrap();
            editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(editor.source_tool_content_view(), protected);
            let attempt = editor
                .reconcile_source_tool_content(&protected.basis)
                .unwrap();
            let pending = completed(editor, &attempt, Outcome::NeedsProtectionPublication);
            assert!(sample(&pending).modified);
            assert!(sample(&pending).adopted);
            assert_eq!(
                fs::read(&path).unwrap(),
                serde_json::to_vec(&rollback).unwrap()
            );
            let repair = editor
                .prepare_source_tool_content_observation(&pending.basis, &id)
                .unwrap();
            let reconciled = commit(editor, repair, Outcome::Observed);
            assert!(sample(&reconciled).modified);
            assert!(sample(&reconciled).adopted);
            assert_record(&project, &reconciled);
            assert_files(&project, &files);
            assert_eq!(source_snapshot(editor), source);
            assert_no_execution(editor);
            expected = reconciled.installations;
            expected[0].creation_custody = false;
            // Project::run closes immediately after the explicit original-basis repair.
        });
        project.run(|editor| {
            let reopened = loaded(editor);
            assert_eq!(reopened.installations, expected);
            assert!(sample(&reopened).modified);
            assert!(sample(&reopened).adopted);
            assert_record(&project, &reopened);
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn reconciliation_refuses_erased_entries_and_changed_immutable_member_facts() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let source = source_snapshot(editor);
            let installation = install(editor);
            let files = file_snapshot(&project, &installation);
            let id = *installation.receipt().installation_id();
            let managed = manage(editor, &installation);
            let adoption = editor
                .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                .unwrap();
            let protected = commit(editor, adoption, Outcome::Adopted);
            let path = project.root.join(RECORD);
            let original = fs::read(&path).unwrap();
            for erase_entry in [true, false] {
                let mut changed: Value = serde_json::from_slice(&original).unwrap();
                if erase_entry {
                    changed["installations"] = json!([]);
                } else {
                    let member = changed["installations"][0]["members"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|member| member["path"] == SAMPLE)
                        .unwrap();
                    let length = member["baseline"]["length"].as_u64().unwrap();
                    member["baseline"]["length"] = json!(length + 1);
                }
                let external = serde_json::to_vec(&changed).unwrap();
                fs::write(&path, &external).unwrap();
                let basis = editor.source_tool_content_view().basis;
                let attempt = editor.reconcile_source_tool_content(&basis).unwrap();
                let refused = completed(editor, &attempt, Outcome::Failed(Error::Changed));
                assert_eq!(refused.installations, protected.installations);
                assert_eq!(fs::read(&path).unwrap(), external);
                assert!(matches!(
                    editor.prepare_source_tool_content_adoption(&refused.basis, &id, &[SAMPLE]),
                    Err(Error::Changed),
                ));
                assert_eq!(editor.source_tool_content_view(), refused);
                fs::write(&path, &original).unwrap();
                let attempt = editor
                    .reconcile_source_tool_content(&refused.basis)
                    .unwrap();
                let restored = completed(editor, &attempt, Outcome::Reconciled);
                assert_eq!(restored.installations, protected.installations);
                assert_eq!(fs::read(&path).unwrap(), original);
                assert_files(&project, &files);
                assert_eq!(source_snapshot(editor), source);
                assert_no_execution(editor);
            }
        });
    }

    #[test]
    fn corrupt_and_missing_ledgers_preserve_files_and_never_reconstruct_creation_custody() {
        let project = Project::new();
        let mut valid = Vec::new();
        let mut files = BTreeMap::new();
        let mut id = [0; 32];
        project.run(|editor| {
            loaded(editor);
            let installation = install(editor);
            id = *installation.receipt().installation_id();
            files = file_snapshot(&project, &installation);
            let managed = manage(editor, &installation);
            let adoption = editor
                .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                .unwrap();
            commit(editor, adoption, Outcome::Adopted);
            valid = fs::read(project.root.join(RECORD)).unwrap();
        });
        fs::write(project.root.join(RECORD), b"{").unwrap();
        project.run(|editor| {
            let unavailable = loaded(editor);
            let source = source_snapshot(editor);
            assert_eq!(unavailable.phase, Phase::Unavailable);
            assert_eq!(unavailable.error, Some(Error::InvalidRecord));
            assert!(unavailable.installations.is_empty());
            assert!(matches!(
                editor.prepare_source_tool_content_adoption(&unavailable.basis, &id, &[SAMPLE]),
                Err(Error::InvalidRecord),
            ));
            assert_eq!(editor.source_tool_content_view(), unavailable);
            let attempt = editor
                .reconcile_source_tool_content(&unavailable.basis)
                .unwrap();
            let failed = completed(editor, &attempt, Outcome::Failed(Error::InvalidRecord));
            assert_eq!(failed.phase, Phase::Unavailable);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), b"{");
            assert_files(&project, &files);
            fs::write(project.root.join(RECORD), &valid).unwrap();
            editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(editor.source_tool_content_view(), failed);
            let attempt = editor.reconcile_source_tool_content(&failed.basis).unwrap();
            let repaired = completed(editor, &attempt, Outcome::Reconciled);
            assert!(sample(&repaired).adopted);
            assert!(!repaired.installations[0].creation_custody);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), valid);
            assert_eq!(source_snapshot(editor), source);
            assert_no_execution(editor);
        });
        fs::remove_file(project.root.join(RECORD)).unwrap();
        project.run(|editor| {
            let missing = loaded(editor);
            assert_eq!(missing.phase, Phase::Idle);
            assert!(missing.installations.is_empty());
            assert!(missing.error.is_none());
            assert!(matches!(
                editor.prepare_source_tool_content_adoption(&missing.basis, &id, &[SAMPLE]),
                Err(Error::Unmanaged),
            ));
            assert!(matches!(
                editor.prepare_source_tool_content_observation(&missing.basis, &id),
                Err(Error::Unmanaged),
            ));
            assert_eq!(editor.source_tool_content_view(), missing);
            let attempt = editor
                .reconcile_source_tool_content(&missing.basis)
                .unwrap();
            let reconciled = completed(editor, &attempt, Outcome::Reconciled);
            assert!(reconciled.installations.is_empty());
            assert!(!project.root.join(RECORD).exists());
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn management_refuses_foreign_roots_and_directory_aliases_before_any_write() {
        let foreign_project = Project::new();
        let mut foreign = None;
        foreign_project.run(|editor| {
            loaded(editor);
            foreign = Some(install(editor));
        });
        let foreign = foreign.unwrap();
        let project = Project::new();
        project.run(|editor| {
            let before = loaded(editor);
            let installation = install(editor);
            let files = file_snapshot(&project, &installation);
            assert!(matches!(
                editor.prepare_source_tool_content_management(
                    &before.basis,
                    DIRECTORY,
                    Arc::clone(&foreign),
                ),
                Err(Error::RootMismatch),
            ));
            assert!(matches!(
                editor.prepare_source_tool_content_management(
                    &before.basis,
                    "INSTALLED-STATS",
                    Arc::clone(&installation),
                ),
                Err(Error::Read),
            ));
            assert!(matches!(
                editor.prepare_source_tool_content_management(
                    &before.basis,
                    "../installed-stats",
                    Arc::clone(&installation),
                ),
                Err(Error::InvalidPath),
            ));
            assert_eq!(editor.source_tool_content_view(), before);
            assert!(!project.root.join(RECORD).exists());
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn protection_management_preserves_dirty_source_history_and_ordinary_save() {
        let project = Project::new();
        let original = project.persisted();
        let mut saved_source = None;
        project.run(|editor| {
            loaded(editor);
            let document = editor.workspace.active_document().unwrap();
            for name in ["first-authored-entity", "second-authored-entity"] {
                let basis = editor.document_query_basis(document).unwrap();
                let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(SceneEntityId::new(name).unwrap()),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]);
                let candidate = editor.prepare_authoring_edit(&basis, &patch).unwrap();
                let result = editor.commit_authoring_edit(candidate);
                assert!(
                    matches!(&result, EditorActionOutcome::Workspace(report) if report.applied),
                    "{result:?}",
                );
            }
            let revision = editor.document_query_basis(document).unwrap().revision;
            let report = editor
                .dispatch_editor_actions(EditorActionBatch::new(
                    editor.current_frame().revision(),
                    [EditorAction::Mutate(EditorDocumentMutation::Undo {
                        document,
                        base_revision: revision,
                    })],
                ))
                .unwrap();
            assert!(matches!(
                report.outcomes(),
                [EditorActionOutcome::Workspace(report)] if report.applied
            ));
            let before = source_snapshot(editor);
            assert!(before.dirty);
            assert_eq!(before.history.undo_depth, 1);
            assert_eq!(before.history.redo_depth, 1);
            let installation = install(editor);
            let managed = manage(editor, &installation);
            let id = *installation.receipt().installation_id();
            let adoption = editor
                .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                .unwrap();
            commit(editor, adoption, Outcome::Adopted);
            assert_eq!(source_snapshot(editor), before);
            assert_eq!(project.persisted(), original);
            let record = fs::read(project.root.join(RECORD)).unwrap();
            let report = editor
                .dispatch_editor_actions(EditorActionBatch::new(
                    editor.current_frame().revision(),
                    [EditorAction::Persistence(EditorPersistenceAction::Save {
                        document,
                        base_revision: before.revision,
                    })],
                ))
                .unwrap();
            let [
                EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted {
                    attempt,
                }),
            ] = report.outcomes()
            else {
                panic!("ordinary dirty Save must remain available: {report:?}");
            };
            let attempt = *attempt;
            wait(editor, |editor| {
                let persistence = editor.persistence_view();
                persistence.attempt() == Some(attempt) && persistence.result().is_some()
            });
            assert!(matches!(
                editor.persistence_view().result(),
                Some(EditorPersistenceResult::Saved { document: saved, revision, .. })
                    if saved == document && revision == before.revision
            ));
            let after = source_snapshot(editor);
            assert_eq!(after.source, before.source);
            assert_eq!(after.revision, before.revision);
            assert_eq!(after.history, before.history);
            assert_eq!(after.usage, before.usage);
            assert!(!after.dirty);
            assert_ne!(project.persisted(), original);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
            assert_no_execution(editor);
            saved_source = Some(after.source);
        });
        project.run(|editor| {
            let reopened = loaded(editor);
            assert_eq!(
                source_snapshot(editor).source,
                *saved_source.as_ref().unwrap()
            );
            assert!(!source_snapshot(editor).dirty);
            assert!(sample(&reopened).adopted);
            assert!(!reopened.installations[0].creation_custody);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn managed_metadata_is_reserved_from_other_document_source_owners() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let installation = install(editor);
            let managed = manage(editor, &installation);
            let source = source_snapshot(editor);
            let record = fs::read(project.root.join(RECORD)).unwrap();
            let root = editor.source_project_root();
            let current = root
                .open_file(&nara_fs::RelativePath::new(RECORD).unwrap())
                .unwrap();
            assert_eq!(
                editor.exclude_source_aliases(RECORD, &current),
                Err(SourceAdmissionError::AlreadyOpen),
            );
            assert_eq!(
                editor.exclude_source_aliases(".NARA/source-tools/CONTENT.JSON", &current),
                Err(SourceAdmissionError::AlreadyOpen),
            );
            assert_eq!(editor.source_tool_content_view(), managed);
            assert_eq!(source_snapshot(editor), source);
            assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
            assert_no_execution(editor);
        });
    }

    #[test]
    fn corrupt_reserved_metadata_and_its_aliases_refuse_document_ownership_without_an_admitted_host()
     {
        let project = Project::new();
        let record = project.root.join(RECORD);
        fs::create_dir_all(record.parent().unwrap()).unwrap();
        fs::write(&record, b"{corrupt management data").unwrap();
        let alias = ".nara/source-tools/project-alias.json";
        fs::hard_link(&record, project.root.join(alias)).unwrap();
        project.run(|editor| {
            let unavailable = loaded(editor);
            assert_eq!(unavailable.phase, Phase::Unavailable);
            assert!(editor.source_tools.content.host.is_none());
            let root = editor.source_project_root();
            let current = root
                .open_file(&nara_fs::RelativePath::new(alias).unwrap())
                .unwrap();
            assert_eq!(
                editor.exclude_source_aliases(".NARA/source-tools/CONTENT.JSON", &current),
                Err(SourceAdmissionError::AlreadyOpen),
            );
            assert!(editor.exclude_source_aliases(alias, &current).is_err());
            assert_eq!(fs::read(&record).unwrap(), b"{corrupt management data");
            assert_eq!(
                fs::read(project.root.join(alias)).unwrap(),
                b"{corrupt management data"
            );
            assert_no_execution(editor);
        });
    }

    #[test]
    fn held_content_io_keeps_actual_task_and_close_pending_until_physical_return() {
        let project = Project::new();
        let mut returned = None;
        let mut files = BTreeMap::new();
        project.run(|editor| {
            loaded(editor);
            let source = source_snapshot(editor);
            let installation = install(editor);
            files = file_snapshot(&project, &installation);
            let id = *installation.receipt().installation_id();
            let managed = manage(editor, &installation);
            let adoption = editor
                .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
                .unwrap();
            let closing_candidate = editor
                .prepare_source_tool_content_observation(&managed.basis, &id)
                .unwrap();
            let (gate, control) = IoGate::new();
            assert!(
                editor
                    .source_tools
                    .content
                    .next_io_gate
                    .replace(gate)
                    .is_none()
            );
            let attempt = editor.commit_source_tool_content(adoption).unwrap();
            let observer = editor
                .source_tools
                .content
                .pending
                .as_ref()
                .unwrap()
                .task
                .as_ref()
                .unwrap()
                .return_observer();
            control.wait_until_entered();
            let publishing = editor.source_tool_content_view();
            assert_eq!(publishing.phase, Phase::Publishing);
            assert!(publishing.physical_work_pending);
            assert!(editor.source_tool_view().has_pending_work());
            assert_eq!(
                editor.source_tool_view().content.as_ref(),
                Some(&publishing)
            );
            assert_eq!(publishing.installations, managed.installations);
            assert_eq!(
                publishing.operation,
                Some(EditorSourceToolContentAttempt {
                    attempt,
                    outcome: None,
                }),
            );
            let disk: Value =
                serde_json::from_slice(&fs::read(project.root.join(RECORD)).unwrap()).unwrap();
            let published_sample = disk["installations"][0]["members"]
                .as_array()
                .unwrap()
                .iter()
                .find(|member| member["path"] == SAMPLE)
                .unwrap();
            assert_eq!(published_sample["adopted"], true);
            assert_eq!(observer.state(), TaskReturnState::Pending);
            assert!(matches!(
                editor.prepare_source_tool_content_observation(&publishing.basis, &id),
                Err(Error::Busy),
            ));
            assert_eq!(
                editor.reconcile_source_tool_content(&publishing.basis),
                Err(Error::Busy),
            );
            for _ in 0..3 {
                let started = Instant::now();
                let close = editor.advance_close();
                assert!(
                    started.elapsed() < Duration::from_secs(2),
                    "Close must poll held content IO without waiting for its worker",
                );
                assert!(matches!(
                    close,
                    EditorProjectCloseProgress::Incomplete { .. }
                ));
                assert!(!close.task_shutdown().is_complete(), "{close:?}");
                assert_eq!(observer.state(), TaskReturnState::Pending);
                assert_eq!(editor.source_tool_content_view(), publishing);
                assert_eq!(source_snapshot(editor), source);
                assert_files(&project, &files);
                assert_no_execution(editor);
            }
            assert_eq!(
                editor.commit_source_tool_content(closing_candidate),
                Err(Error::Closing),
            );
            assert_eq!(
                editor.reconcile_source_tool_content(&publishing.basis),
                Err(Error::Closing),
            );
            control.release();
            returned = Some(observer);
            // Project::run performs actual Close and checks the Tasks/cleanup completion receipt.
        });
        assert_eq!(returned.unwrap().state(), TaskReturnState::Returned);
        assert_files(&project, &files);
        project.run(|editor| {
            let reopened = loaded(editor);
            assert!(sample(&reopened).adopted);
            assert!(!reopened.installations[0].creation_custody);
            assert_files(&project, &files);
            assert_no_execution(editor);
        });
    }
}
