//! Actual incomplete publication custody, explicit registration and finite reclamation.

use std::fs;

use nara_fs::ContentDigest;
use nara_tooling::{
    EditorSourceToolArtifactError as ArtifactError,
    EditorSourceToolArtifactObservation as Observation,
    EditorSourceToolArtifactOutcome as ArtifactOutcome,
    EditorSourceToolArtifactPublication as Publication,
};

use super::super::*;
use super::session::{artifact_completed, completed, disk, entered, gate, quiet, writable_root};
use crate::project_host::runtime::editor::source_tools::{
    EditorSourceToolIncompletePublication, EditorSourceToolInstallationOutcome,
    EditorSourceToolInstallationPlan, PreparedEditorSourceToolRelease,
    artifacts::ARTIFACTS_PATH,
    release::{PublicationTestFault, tests::Release},
    tests::fixture::Project,
};

fn publish_partial(
    project: &Project,
    name: &str,
    member: usize,
) -> EditorSourceToolIncompletePublication {
    publish_fault(project, name, PublicationTestFault::MemberCreated(member))
}

fn publish_fault(
    project: &Project,
    name: &str,
    fault: PublicationTestFault,
) -> EditorSourceToolIncompletePublication {
    let release = Release::new();
    let captured = PreparedEditorSourceToolRelease::capture(
        &release.root(nara_fs::TrustMode::Untrusted),
        release.write_manifest(),
    )
    .unwrap();
    let plan = EditorSourceToolInstallationPlan::new(
        writable_root(project),
        nara_fs::RelativeComponent::new(name).unwrap(),
        captured,
    )
    .unwrap();
    match plan.publish_failing_for_tests(fault) {
        EditorSourceToolInstallationOutcome::Incomplete {
            plan, publication, ..
        } => {
            assert_eq!(publication.project_path(), plan.project_path());
            assert_ne!(publication.id(), &[0; 32]);
            assert!(
                publication
                    .project_root()
                    .same_object(&writable_root(project))
                    .unwrap()
            );
            assert_eq!(
                publication.publication().published_identity(),
                publication.directory().identity(),
            );
            publication
        }
        other => panic!("expected an actual partial publication: {other:?}"),
    }
}

fn inventory_bytes(project: &Project) -> Option<Vec<u8>> {
    match fs::read(project.root.join(ARTIFACTS_PATH)) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("cannot read actual inventory: {error}"),
    }
}

fn review(
    editor: &mut EditorProjectSession,
    project: &Project,
    artifact: &[u8; 32],
) -> PreparedEditorSourceToolReclamation {
    quiet(editor);
    editor
        .prepare_source_tool_reclamation(
            &editor.source_tool_artifact_view().basis,
            &editor.source_tool_selection_view().basis,
            writable_root(project),
            artifact,
        )
        .unwrap()
}

#[test]
fn partial_publication_records_actual_intent_unlinks_and_results_before_fresh_open() {
    let project = Project::new();
    let mut saved = None;
    project.run(|editor| {
        quiet(editor);
        let publication = publish_partial(&project, "incomplete-partial", 1);
        let id = *publication.id();
        let relative = publication.project_path().to_owned();
        let directory = project.root.join(&relative);
        let root_observation = writable_root(&project)
            .open_directory_exact(&nara_fs::RelativePath::new(&relative).unwrap())
            .unwrap();
        assert_eq!(publication.members().len(), 2);
        let paths: Vec<_> = publication
            .members()
            .iter()
            .map(|member| member.path().to_owned())
            .collect();
        let bytes: Vec<_> = paths
            .iter()
            .map(|path| fs::read(directory.join(path)).unwrap())
            .collect();
        let manifest = paths
            .iter()
            .position(|path| path == "nara-tool.json")
            .unwrap();
        assert!(!bytes[manifest].is_empty());
        assert_eq!(bytes.iter().filter(|bytes| bytes.is_empty()).count(), 1);
        assert!(!directory.join(".nara-installation.json").exists());
        assert!(!directory.join("samples/readme.txt").exists());
        fs::write(directory.join("unknown-sibling.txt"), b"unowned sibling").unwrap();
        let before = inventory_bytes(&project);
        let prepared = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap();
        assert_eq!(inventory_bytes(&project), before);
        let facts = prepared.artifacts();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, id);
        assert_eq!(facts[0].publication, Publication::Incomplete);
        assert_eq!(facts[0].members.len(), 2);
        assert!(facts[0].members.iter().any(|member| {
            member.path.as_ref() == "nara-tool.json"
                && member.observation == Observation::Present
                && member.baseline == ContentDigest::of_bytes(&bytes[manifest])
        }));
        assert!(facts[0].members.iter().any(|member| {
            member.observation == Observation::Changed
                && fs::read(directory.join(member.path.as_ref()))
                    .unwrap()
                    .is_empty()
        }));
        for (path, original) in paths.iter().zip(&bytes) {
            assert_eq!(fs::read(directory.join(path)).unwrap(), *original);
        }
        assert!(disk_if_present(&project).is_empty());
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        artifact_completed(editor, &attempt, ArtifactOutcome::Registered);
        let prepared = review(editor, &project, &id);
        assert!(prepared.creation_custody());
        assert!(prepared.directory.same_object(&root_observation).unwrap());
        let intent = prepared.record().clone();
        assert_eq!(intent.artifact, id);
        assert_eq!(intent.phase, RecordPhase::Intent);
        assert_eq!(intent.members.len(), 2);
        assert!(
            intent
                .members
                .iter()
                .all(|member| member.state == MemberState::Pending)
        );
        let control = gate(editor, 0);
        editor.confirm_source_tool_reclamation(prepared).unwrap();
        entered(editor, &control, 0);
        assert_eq!(disk(&project), vec![intent.clone()]);
        assert!(!directory.join(intent.members[0].path.as_ref()).exists());
        let later = intent.members[1].path.as_ref();
        let index = paths.iter().position(|path| path == later).unwrap();
        assert_eq!(fs::read(directory.join(later)).unwrap(), bytes[index]);
        assert_eq!(
            fs::read(directory.join("unknown-sibling.txt")).unwrap(),
            b"unowned sibling"
        );
        drop(control);
        let actual = completed(editor, Outcome::Complete);
        assert_eq!(actual.id, intent.id);
        assert_eq!(actual.artifact, id);
        assert_eq!(actual.phase, RecordPhase::Results);
        assert!(
            actual
                .members
                .iter()
                .all(|member| member.state == MemberState::Removed)
        );
        assert_eq!(disk(&project), vec![actual.clone()]);
        for member in &actual.members {
            assert!(!directory.join(member.path.as_ref()).exists());
        }
        assert!(
            editor
                .source_tools
                .artifacts
                .creation_custody(&id)
                .is_none()
        );
        saved = Some((relative, actual, inventory_bytes(&project)));
    });
    let (relative, actual, inventory) = saved.unwrap();
    project.run(|editor| {
        quiet(editor);
        let view = editor.source_tool_artifact_view();
        assert_eq!(view.artifacts[0].publication, Publication::Incomplete);
        assert_eq!(view.reclamations, vec![actual]);
        assert_eq!(editor.source_tool_reclamation_view().phase, Phase::Idle);
        assert!(editor.source_tool_reclamation_view().attempt.is_none());
        for _ in 0..3 {
            editor.drive_editor_frame(std::time::Duration::ZERO);
        }
        assert_eq!(inventory_bytes(&project), inventory);
        assert_eq!(
            fs::read(project.root.join(relative).join("unknown-sibling.txt")).unwrap(),
            b"unowned sibling",
        );
        assert!(editor.source_tools.active.is_none());
    });
}

fn disk_if_present(project: &Project) -> Vec<Record> {
    if inventory_bytes(project).is_some() {
        disk(project)
    } else {
        Vec::new()
    }
}

#[test]
fn cleanup_only_equal_byte_replacement_returns_owner_until_original_name_is_repaired() {
    let project = Project::new();
    project.run(|editor| {
        quiet(editor);
        let publication = publish_partial(&project, "incomplete-repair", 0);
        let id = *publication.id();
        let directory_identity = publication.directory().identity();
        assert_eq!(publication.members().len(), 1);
        assert_eq!(publication.pending_files(), 1);
        let path = publication.members()[0].path().to_owned();
        let directory = project.root.join(publication.project_path());
        let original = directory.join(&path);
        let held = directory.join("original-held.json");
        let replacement = directory.join("unowned-replacement.json");
        let bytes = fs::read(&original).unwrap();
        assert!(bytes.is_empty());
        let before = inventory_bytes(&project);
        fs::rename(&original, &held).unwrap();
        fs::write(&original, &bytes).unwrap();
        let error = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap_err();
        assert_eq!(error.error(), &ArtifactError::Changed);
        let (error, publication) = error.into_parts();
        assert_eq!(error, ArtifactError::Changed);
        assert_eq!(publication.id(), &id);
        assert_eq!(publication.directory().identity(), directory_identity);
        assert_eq!(publication.pending_files(), 1);
        assert_eq!(fs::read(&original).unwrap(), bytes);
        assert_eq!(fs::read(&held).unwrap(), bytes);
        assert_eq!(inventory_bytes(&project), before);
        assert!(disk_if_present(&project).is_empty());
        assert!(editor.source_tool_artifact_view().artifacts.is_empty());
        fs::rename(&original, &replacement).unwrap();
        // A missing original name still cannot hand cleanup over to a reader.
        let error = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap_err();
        assert_eq!(error.error(), &ArtifactError::Changed);
        let (_, publication) = error.into_parts();
        assert_eq!(publication.pending_files(), 1);
        assert_eq!(fs::read(&held).unwrap(), bytes);
        assert_eq!(inventory_bytes(&project), before);
        fs::rename(&held, &original).unwrap();
        let prepared = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap();
        let publication = prepared.into_incomplete_publication().unwrap();
        assert_eq!(publication.id(), &id);
        assert_eq!(publication.pending_files(), 0);
        assert_eq!(publication.members().len(), 1);
        assert_eq!(fs::read(&original).unwrap(), bytes);
        assert_eq!(fs::read(&replacement).unwrap(), bytes);
        assert_eq!(inventory_bytes(&project), before);
        assert!(disk_if_present(&project).is_empty());
        let prepared = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        artifact_completed(editor, &attempt, ArtifactOutcome::Registered);
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let prepared = review(editor, &project, &id);
        assert!(prepared.creation_custody());
        assert_eq!(prepared.record().members.len(), 1);
        editor.confirm_source_tool_reclamation(prepared).unwrap();
        let actual = completed(editor, Outcome::Complete);
        assert_eq!(actual.members[0].state, MemberState::Removed);
        assert!(!original.exists());
        assert_eq!(fs::read(replacement).unwrap(), bytes);
        assert_eq!(disk(&project), vec![actual]);
    });
}

#[test]
fn stale_confirmation_and_failed_registration_preserve_original_incomplete_custody_for_retry() {
    let project = Project::new();
    project.run(|editor| {
        quiet(editor);
        let publication = publish_partial(&project, "incomplete-retained-retry", 0);
        let id = *publication.id();
        let identity = publication.directory().identity();
        let root_observation = writable_root(&project)
            .open_directory_exact(&nara_fs::RelativePath::new(publication.project_path()).unwrap())
            .unwrap();
        let original = project
            .root
            .join(publication.project_path())
            .join("nara-tool.json");
        let bytes = fs::read(&original).unwrap();
        fs::write(
            project.root.join(ARTIFACTS_PATH),
            br#"{"format_version":1,"artifacts":[],"reclamations":[]}"#,
        )
        .unwrap();
        let loaded = editor
            .reconcile_source_tool_artifacts(&editor.source_tool_artifact_view().basis)
            .unwrap();
        artifact_completed(editor, &loaded, ArtifactOutcome::Reconciled);
        let before = inventory_bytes(&project);
        let prepared = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap();
        let facts = prepared.artifacts();
        let reconcile = editor
            .reconcile_source_tool_artifacts(&editor.source_tool_artifact_view().basis)
            .unwrap();
        artifact_completed(editor, &reconcile, ArtifactOutcome::Reconciled);
        let error = editor.commit_source_tool_artifacts(prepared).unwrap_err();
        assert_eq!(error.error(), &ArtifactError::StaleBasis);
        let (error, prepared) = error.into_parts();
        assert_eq!(error, ArtifactError::StaleBasis);
        assert_eq!(prepared.artifacts(), facts);
        assert_eq!(inventory_bytes(&project), before);
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let publication = prepared.into_incomplete_publication().unwrap();
        assert_eq!(publication.id(), &id);
        assert_eq!(publication.directory().identity(), identity);
        assert_eq!(publication.pending_files(), 0);
        let prepared = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap();
        let failure = editor.source_tools.artifacts.test_fail_next_publication();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        crate::project_host::runtime::editor::source_tools::tests::fixture::wait(
            editor,
            |editor| editor.source_tool_artifact_view().error.is_some(),
        );
        assert_eq!(fs::read(&original).unwrap(), bytes);
        assert_eq!(inventory_bytes(&project), before);
        assert!(
            editor
                .source_tools
                .artifacts
                .creation_custody(&id)
                .is_some()
        );
        failure.release();
        editor
            .source_tools
            .artifacts
            .test_allow_publication_cleanup();
        artifact_completed(
            editor,
            &attempt,
            ArtifactOutcome::Failed(ArtifactError::Write),
        );
        let reconcile = editor
            .reconcile_source_tool_artifacts(&editor.source_tool_artifact_view().basis)
            .unwrap();
        artifact_completed(editor, &reconcile, ArtifactOutcome::Reconciled);
        assert!(editor.source_tool_artifact_view().artifacts.is_empty());
        assert_eq!(inventory_bytes(&project), before);
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let prepared = editor
            .prepare_retained_source_tool_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                &id,
            )
            .unwrap();
        assert_eq!(prepared.artifacts(), facts);
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        let registered = artifact_completed(editor, &attempt, ArtifactOutcome::Registered);
        assert_eq!(registered.artifacts, facts);
        assert!(registered.reclamations.is_empty());
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let prepared = review(editor, &project, &id);
        assert!(prepared.creation_custody());
        assert!(prepared.directory.same_object(&root_observation).unwrap());
        editor.confirm_source_tool_reclamation(prepared).unwrap();
        let actual = completed(editor, Outcome::Complete);
        assert_eq!(actual.artifact, id);
        assert_eq!(actual.members.len(), 1);
        assert_eq!(actual.members[0].state, MemberState::Removed);
        assert!(!original.exists());
        assert_eq!(disk(&project), vec![actual]);
    });
}

#[test]
fn zero_member_and_receipt_competition_publications_use_the_same_reclaimer() {
    for fault in [
        PublicationTestFault::BeforePayloads,
        PublicationTestFault::CompetingReceipt,
    ] {
        let project = Project::new();
        project.run(|editor| {
            quiet(editor);
            let publication = publish_fault(&project, "unready-boundary", fault);
            let id = *publication.id();
            let directory = project.root.join(publication.project_path());
            let empty = fault == PublicationTestFault::BeforePayloads;
            assert_eq!(publication.members().is_empty(), empty);
            let actual_paths: Vec<_> = publication
                .members()
                .iter()
                .map(|member| member.path().to_owned())
                .collect();
            assert!(
                !actual_paths
                    .iter()
                    .any(|path| path == ".nara-installation.json")
            );
            if !empty {
                assert!(publication.members().iter().any(|member| member.role()
                    == nara_tooling::EditorSourceToolArtifactRole::ReceiptTemporary));
            }
            fs::write(directory.join("unknown.txt"), b"unowned").unwrap();
            let prepared = editor
                .prepare_source_tool_incomplete_artifact_registration(
                    &editor.source_tool_artifact_view().basis,
                    publication,
                )
                .unwrap();
            let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
            artifact_completed(editor, &attempt, ArtifactOutcome::Registered);
            let prepared = review(editor, &project, &id);
            assert_eq!(prepared.record().members.len(), actual_paths.len());
            editor.confirm_source_tool_reclamation(prepared).unwrap();
            let actual = completed(editor, Outcome::Complete);
            assert_eq!(actual.members.len(), actual_paths.len());
            assert!(
                actual
                    .members
                    .iter()
                    .all(|member| member.state == MemberState::Removed)
            );
            for path in &actual_paths {
                assert!(!directory.join(path).exists());
            }
            assert_eq!(fs::read(directory.join("unknown.txt")).unwrap(), b"unowned");
            if !empty {
                assert_eq!(
                    fs::read(directory.join(".nara-installation.json")).unwrap(),
                    b"independent competing receipt"
                );
            }
            assert_eq!(disk(&project), vec![actual]);
        });
    }
}

#[test]
fn incomplete_cancellation_and_failed_results_retain_mixed_facts_without_continuing_unlink() {
    let project = Project::new();
    project.run(|editor| {
        quiet(editor);
        let publication = publish_partial(&project, "unready-cancel-results", 2);
        let id = *publication.id();
        let directory = project.root.join(publication.project_path());
        let prepared = editor
            .prepare_source_tool_incomplete_artifact_registration(
                &editor.source_tool_artifact_view().basis,
                publication,
            )
            .unwrap();
        let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
        artifact_completed(editor, &attempt, ArtifactOutcome::Registered);
        let prepared = review(editor, &project, &id);
        let control = gate(editor, 0);
        let attempt = editor.confirm_source_tool_reclamation(prepared).unwrap();
        entered(editor, &control, 0);
        editor.cancel_source_tool_reclamation(&attempt).unwrap();
        let failure = editor.source_tools.artifacts.test_fail_next_publication();
        drop(control);
        crate::project_host::runtime::editor::source_tools::tests::fixture::wait(
            editor,
            |editor| editor.source_tool_artifact_view().error.is_some(),
        );
        failure.release();
        editor
            .source_tools
            .artifacts
            .test_allow_publication_cleanup();
        crate::project_host::runtime::editor::source_tools::tests::fixture::wait(
            editor,
            |editor| editor.source_tool_reclamation_view().phase == Phase::Terminal,
        );
        let failed = editor.source_tool_reclamation_view();
        assert!(failed.record_publication_pending && !failed.physical_work_pending);
        let actual = failed.record.unwrap();
        assert_eq!(actual.members[0].state, MemberState::Removed);
        assert!(
            actual.members[1..]
                .iter()
                .all(|member| member.state == MemberState::Pending)
        );
        assert_eq!(disk(&project)[0].phase, RecordPhase::Intent);
        let before: Vec<_> = actual.members[1..]
            .iter()
            .map(|member| {
                (
                    member.path.clone(),
                    fs::read(directory.join(member.path.as_ref())).unwrap(),
                )
            })
            .collect();
        let reconcile = editor
            .reconcile_source_tool_artifacts(&editor.source_tool_artifact_view().basis)
            .unwrap();
        let reconciled = artifact_completed(editor, &reconcile, ArtifactOutcome::Reconciled);
        editor
            .retry_source_tool_reclamation_record(&attempt, &reconciled.basis)
            .unwrap();
        assert_eq!(completed(editor, Outcome::Cancelled), actual);
        for (path, bytes) in before {
            assert_eq!(fs::read(directory.join(path.as_ref())).unwrap(), bytes);
        }
        assert_eq!(disk(&project), vec![actual]);
    });
}
