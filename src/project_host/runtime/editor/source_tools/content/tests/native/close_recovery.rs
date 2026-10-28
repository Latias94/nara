//! Actual task retirement and first-management publication recovery.

use super::super::WAIT;
use super::*;
use crate::project_host::runtime::editor::test_fixtures::HeldCompute;

#[test]
fn retired_content_host_does_not_refence_close_while_an_unrelated_task_returns() {
    let project = Project::new();
    project.run(|editor| {
        loaded(editor);
        let installation = install(editor);
        let managed = manage(editor, &installation);
        let files = file_snapshot(&project, &installation);
        let record = fs::read(project.root.join(RECORD)).unwrap();
        let host = editor.source_tools.content.host.as_ref().unwrap().clone();
        let mut held = HeldCompute::start(editor.authoring_runtime.persistence_tasks(), || {});
        let deadline = Instant::now() + WAIT;

        loop {
            let started = Instant::now();
            let progress = editor.advance_close();
            assert!(started.elapsed() < Duration::from_secs(2), "{progress:?}");
            assert!(
                matches!(progress, EditorProjectCloseProgress::Incomplete { .. }),
                "{progress:?}",
            );
            assert!(!progress.task_shutdown().is_complete(), "{progress:?}");
            assert_eq!(held.returned.state(), TaskReturnState::Pending);
            assert!(editor.closing);
            assert!(!editor.authoring_runtime.host_custody().is_accepting());
            if host.is_retired() {
                break;
            }
            assert!(Instant::now() < deadline, "content Host did not retire");
            std::thread::sleep(Duration::from_millis(1));
        }

        let content = editor.source_tool_content_view();
        assert_eq!(content.installations, managed.installations);
        assert!(!content.protection_publication_pending);
        assert!(!content.physical_work_pending, "{content:?}");

        // A separate cancellation cannot return the executor-owned late result's Drop.
        held.handle.cancel();
        held.release_body_until_drop();
        assert_eq!(held.returned.state(), TaskReturnState::Pending);
        let progress = editor.advance_close();
        assert!(
            matches!(progress, EditorProjectCloseProgress::Incomplete { .. }),
            "{progress:?}",
        );
        assert!(!progress.task_shutdown().is_complete(), "{progress:?}");
        assert!(host.is_retired());
        assert_eq!(held.returned.state(), TaskReturnState::Pending);

        held.release_and_wait(TaskReturnState::Returned);
        let deadline = Instant::now() + WAIT;
        loop {
            let progress = editor.advance_close();
            if let EditorProjectCloseProgress::Complete(receipt) = progress {
                assert!(receipt.task_shutdown().is_complete());
                assert!(!receipt.has_cleanup_failure());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Close did not resume after actual task return: {progress:?}",
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(host.is_retired());
        assert_eq!(held.returned.state(), TaskReturnState::Returned);
        assert_eq!(fs::read(project.root.join(RECORD)).unwrap(), record);
        assert_files(&project, &files);
    });
}

#[test]
fn failed_first_management_can_reconcile_missing_entry_and_publish_explicit_protection() {
    let project = Project::new();
    let empty = br#"{"format_version":1,"installations":[]}"#;
    let path = project.root.join(RECORD);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, empty).unwrap();
    let mut persisted = None;

    project.run(|editor| {
        let initial = loaded(editor);
        assert_eq!(initial.phase, Phase::Idle);
        assert!(initial.installations.is_empty());
        let original_host = editor.source_tools.content.host.as_ref().unwrap().clone();
        let original_observation = original_host
            .try_with(|host| host.accepted_source_observation())
            .unwrap();
        assert!(original_observation.is_some());
        let source = source_snapshot(editor);
        let installation = install(editor);
        fs::write(
            project.root.join(DIRECTORY).join(SAMPLE),
            b"project-authored content before management",
        )
        .unwrap();
        let files = file_snapshot(&project, &installation);
        let candidate = editor
            .prepare_source_tool_content_management(
                &initial.basis,
                DIRECTORY,
                Arc::clone(&installation),
            )
            .unwrap();
        let confirmed = candidate.installations();
        assert_eq!(confirmed.len(), 1);
        assert_facts(&confirmed[0], &installation);
        assert!(confirmed[0].creation_custody);
        assert!(
            confirmed[0]
                .members
                .iter()
                .any(|member| member.path.as_ref() == SAMPLE && member.modified)
        );
        original_host
            .try_with(|host| {
                let cleanup = host.test_fail_after_write();
                cleanup.release();
            })
            .unwrap();

        let failed = commit(editor, candidate, Outcome::Failed(Error::Write));
        assert_eq!(failed.phase, Phase::RequiresReconcile);
        assert_eq!(failed.installations, confirmed);
        assert!(failed.protection_publication_pending);
        assert!(!failed.physical_work_pending);
        assert_eq!(fs::read(&path).unwrap(), empty);
        assert!(matches!(
            editor.prepare_source_tool_content_protection(&failed.basis),
            Err(Error::RequiresReconcile),
        ));
        let progress = editor.advance_close();
        assert!(
            matches!(
                progress,
                EditorProjectCloseProgress::Incomplete {
                    file_cleanup_pending: true,
                    ..
                }
            ),
            "{progress:?}",
        );
        assert!(!editor.closing);
        assert!(editor.authoring_runtime.host_custody().is_accepting());

        let attempt = editor.reconcile_source_tool_content(&failed.basis).unwrap();
        let reconciled = completed(editor, &attempt, Outcome::NeedsProtectionPublication);
        assert_eq!(reconciled.installations, confirmed);
        assert_eq!(fs::read(&path).unwrap(), empty);
        let reconciled_host = editor.source_tools.content.host.as_ref().unwrap();
        assert!(!reconciled_host.same_owner(&original_host));
        assert!(original_host.is_retired());
        assert_eq!(
            reconciled_host
                .try_with(|host| host.accepted_source_observation())
                .unwrap(),
            original_observation,
            "reconciliation must retain the actual empty disk baseline for repair",
        );

        let repair = editor
            .prepare_source_tool_content_protection(&reconciled.basis)
            .unwrap();
        assert_eq!(repair.installations(), confirmed);
        let repaired = commit(editor, repair, Outcome::Protected);
        assert_eq!(repaired.installations, confirmed);
        assert!(sample(&repaired).modified);
        assert_record(&project, &repaired);
        assert_eq!(source_snapshot(editor), source);
        assert_files(&project, &files);
        assert_no_execution(editor);
        let mut expected = repaired.installations;
        expected[0].creation_custody = false;
        persisted = Some((expected, fs::read(&path).unwrap(), files));
    });

    let (expected, record, files) = persisted.unwrap();
    project.run(|editor| {
        let reopened = loaded(editor);
        assert_eq!(reopened.phase, Phase::Idle);
        assert_eq!(reopened.installations, expected);
        assert!(sample(&reopened).modified);
        assert!(!reopened.installations[0].creation_custody);
        assert!(!reopened.protection_publication_pending);
        assert!(!reopened.physical_work_pending);
        assert_record(&project, &reopened);
        assert_eq!(fs::read(&path).unwrap(), record);
        assert_files(&project, &files);
        assert_no_execution(editor);
    });
}
