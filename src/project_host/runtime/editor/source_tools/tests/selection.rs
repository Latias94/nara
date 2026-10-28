//! Durable selection observations through real project Sessions and finite child tools.

use std::fs;

use nara_fs::ContentDigest;
use nara_scene::{SceneDocument, SceneEntityRecord};
use nara_tooling::{
    EditorSourceToolDesiredSelection, EditorSourceToolInstalledSelection,
    EditorSourceToolSelectionAttempt, EditorSourceToolSelectionError as Error,
    EditorSourceToolSelectionOutcome as Outcome, EditorSourceToolSelectionPhase as Phase,
    EditorSourceToolSelectionView,
};
use serde_json::{Value, json};

use super::*;

const RECORD_PATH: &str = ".nara/source-tools/selection.json";

fn desired(enabled: bool) -> EditorSourceToolDesiredSelection {
    EditorSourceToolDesiredSelection {
        installed: EditorSourceToolInstalledSelection {
            package: "test.finite-source-payload".into(),
            directory: "tools/finite-source-payload".into(),
            installation: [1; 32],
            receipt: ContentDigest::from_parts(32, [2; 32]),
            manifest: ContentDigest::from_parts(64, [3; 32]),
        },
        enabled,
    }
}

fn record(enabled: bool) -> Value {
    let installed = desired(enabled).installed;
    // Construct the wire record independently of the production store encoder.
    json!({
        "format_version": 1,
        "installed": {
            "package": installed.package.as_ref(),
            "directory": installed.directory.as_ref(),
            "installation": installed.installation,
            "receipt": { "length": 32, "hash": *installed.receipt.as_bytes() },
            "manifest": { "length": 64, "hash": *installed.manifest.as_bytes() },
        },
        "desired_enabled": enabled,
    })
}

fn write_record(project: &Project, bytes: &[u8]) {
    let path = project.root.join(RECORD_PATH);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn loaded_selection(editor: &mut EditorProjectSession) -> EditorSourceToolSelectionView {
    wait(editor, |editor| {
        let view = editor.source_tool_selection_view();
        matches!(view.phase, Phase::Idle | Phase::Unavailable)
            && view
                .operation
                .as_ref()
                .is_some_and(|operation| operation.outcome.is_some())
    });
    editor.source_tool_selection_view()
}

#[test]
fn enabled_selection_loads_without_execution_and_disable_survives_reopen() {
    let project = Project::new();
    let enabled = serde_json::to_vec(&record(true)).unwrap();
    write_record(&project, &enabled);
    let persisted = project.persisted();
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(target())]);
    assert!(
        !project
            .root
            .join(desired(true).installed.directory.as_ref())
            .exists()
    );

    project.run(|editor| {
        let loaded = loaded_selection(editor);
        assert_eq!(loaded.phase, Phase::Idle);
        assert_eq!(loaded.desired, Some(desired(true)));
        assert_eq!(
            loaded.operation.as_ref().unwrap().outcome,
            Some(Outcome::Observed),
        );
        assert!(loaded.disable.is_none());
        assert!(loaded.error.is_none());
        assert!(loaded.active_origin.is_none());
        let tool = editor.source_tool_view();
        assert!(tool.active.is_none());
        assert!(!tool.probing);
        assert!(tool.attempt.is_none());
        let before = snapshot(editor);
        assert_eq!(before.source, source);
        assert_eq!(fs::read(project.root.join(RECORD_PATH)).unwrap(), enabled);

        let attempt = editor.disable_installed_source_tool(&loaded.basis).unwrap();
        wait(editor, |editor| {
            let view = editor.source_tool_selection_view();
            view.phase == Phase::Idle
                && view
                    .disable
                    .as_ref()
                    .is_some_and(|disable| disable.attempt == attempt && disable.outcome.is_some())
        });
        let disabled = editor.source_tool_selection_view();
        assert_eq!(disabled.phase, Phase::Idle);
        assert_eq!(disabled.desired, Some(desired(false)));
        assert_eq!(disabled.operation, loaded.operation);
        assert_eq!(
            disabled.disable,
            Some(EditorSourceToolSelectionAttempt {
                attempt,
                outcome: Some(Outcome::Disabled),
            }),
        );
        assert!(disabled.error.is_none());
        assert!(disabled.active_origin.is_none());
        let bytes = fs::read(project.root.join(RECORD_PATH)).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            record(false)
        );

        assert_eq!(
            editor.disable_installed_source_tool(&loaded.basis),
            Err(Error::StaleBasis)
        );
        assert_eq!(editor.source_tool_selection_view(), disabled);
        assert!(editor.source_tool_view().active.is_none());
        assert!(!editor.source_tool_view().probing);
        assert_eq!(fs::read(project.root.join(RECORD_PATH)).unwrap(), bytes);
        assert_eq!(snapshot(editor), before);
        assert_eq!(project.persisted(), persisted);
    });

    project.run(|editor| {
        let loaded = loaded_selection(editor);
        assert_eq!(loaded.phase, Phase::Idle);
        assert_eq!(loaded.desired, Some(desired(false)));
        assert_eq!(
            loaded.operation.as_ref().unwrap().outcome,
            Some(Outcome::Observed),
        );
        assert!(loaded.disable.is_none());
        assert!(loaded.error.is_none());
        assert!(loaded.active_origin.is_none());
        let tool = editor.source_tool_view();
        assert!(tool.active.is_none());
        assert!(!tool.probing);
        assert!(tool.attempt.is_none());
        assert_eq!(snapshot(editor).source, source);
        assert_eq!(project.persisted(), persisted);
    });
}

#[test]
fn unavailable_selection_preserves_bytes_until_explicit_code_free_reconcile() {
    let mut unsupported = record(true);
    unsupported["format_version"] = json!(2);
    for (bytes, error) in [
        (b"{".to_vec(), Error::InvalidRecord),
        (
            serde_json::to_vec(&unsupported).unwrap(),
            Error::UnsupportedFormat,
        ),
    ] {
        let project = Project::new();
        write_record(&project, &bytes);
        let persisted = project.persisted();
        project.run(|editor| {
            let loaded = loaded_selection(editor);
            assert_eq!(loaded.phase, Phase::Unavailable);
            assert!(loaded.desired.is_none());
            assert!(loaded.active_origin.is_none());
            assert_eq!(loaded.error, Some(error.clone()));
            assert_eq!(
                loaded.operation.as_ref().unwrap().outcome,
                Some(Outcome::Failed(error.clone())),
            );
            assert!(editor.source_tool_view().active.is_none());
            assert!(!editor.source_tool_view().probing);
            assert!(editor.source_tool_view().attempt.is_none());
            let before = snapshot(editor);

            let activation = activate(editor, &project);
            assert_eq!(
                editor.source_tool_view().active.as_ref().unwrap().id(),
                &activation
            );
            assert_eq!(
                editor.source_tool_selection_view().phase,
                Phase::Unavailable
            );
            let basis = editor.source_tool_selection_view().basis;
            let attempt = editor.disable_installed_source_tool(&basis).unwrap();
            let stopped = editor.source_tool_selection_view();
            assert_eq!(stopped.phase, Phase::Unavailable);
            assert_eq!(stopped.operation, loaded.operation);
            assert_eq!(
                stopped.disable,
                Some(EditorSourceToolSelectionAttempt {
                    attempt,
                    outcome: Some(Outcome::Failed(error.clone())),
                }),
            );
            assert!(editor.source_tool_view().active.is_none());
            assert_eq!(fs::read(project.root.join(RECORD_PATH)).unwrap(), bytes);
            let probe_attempt = editor.source_tool_view().attempt;

            let valid = serde_json::to_vec(&record(true)).unwrap();
            write_record(&project, &valid);
            editor.drive_editor_frame(Duration::ZERO);
            assert_eq!(editor.source_tool_selection_view(), stopped);
            let attempt = editor
                .reconcile_source_tool_selection(&stopped.basis)
                .unwrap();
            wait(editor, |editor| {
                let view = editor.source_tool_selection_view();
                view.phase == Phase::Idle
                    && view.operation.as_ref().is_some_and(|operation| {
                        operation.attempt == attempt && operation.outcome.is_some()
                    })
            });
            let reconciled = editor.source_tool_selection_view();
            assert_eq!(reconciled.phase, Phase::Idle);
            assert_eq!(reconciled.desired, Some(desired(true)));
            assert_eq!(
                reconciled.operation,
                Some(EditorSourceToolSelectionAttempt {
                    attempt,
                    outcome: Some(Outcome::Reconciled),
                }),
            );
            assert_eq!(reconciled.disable, stopped.disable);
            assert!(reconciled.error.is_none());
            assert!(reconciled.active_origin.is_none());
            let tool = editor.source_tool_view();
            assert!(tool.active.is_none());
            assert!(!tool.probing);
            assert_eq!(tool.attempt, probe_attempt);
            assert_eq!(fs::read(project.root.join(RECORD_PATH)).unwrap(), valid);
            assert_eq!(snapshot(editor), before);
            assert_eq!(project.persisted(), persisted);
        });
    }
}

#[test]
fn selection_basis_before_session_only_replacement_cannot_disable_successor() {
    let project = Project::new();
    let persisted = project.persisted();
    project.run(|editor| {
        assert_eq!(loaded_selection(editor).phase, Phase::Idle);
        let predecessor = activate(editor, &project);
        let captured = editor.source_tool_selection_view();
        let before = snapshot(editor);
        let successor = activate(editor, &project);
        assert_ne!(successor, predecessor);
        let current = editor.source_tool_selection_view();
        assert_ne!(current.basis, captured.basis);
        assert_eq!(
            editor.disable_installed_source_tool(&captured.basis),
            Err(Error::StaleBasis)
        );
        assert_eq!(editor.source_tool_selection_view(), current);
        assert_eq!(
            editor.source_tool_view().active.as_ref().unwrap().id(),
            &successor
        );
        assert!(!editor.source_tool_view().probing);
        assert!(current.desired.is_none());
        assert!(current.active_origin.is_none());
        assert!(!project.root.join(RECORD_PATH).exists());
        assert_eq!(snapshot(editor), before);
        assert_eq!(project.persisted(), persisted);
    });
}

#[test]
fn changed_selection_stops_execution_without_overwriting_or_reactivation() {
    let project = Project::new();
    write_record(&project, &serde_json::to_vec(&record(true)).unwrap());
    let persisted = project.persisted();
    project.run(|editor| {
        let loaded = loaded_selection(editor);
        assert_eq!(loaded.phase, Phase::Idle);
        assert_eq!(loaded.desired, Some(desired(true)));
        let before = snapshot(editor);
        let external = serde_json::to_vec_pretty(&record(false)).unwrap();
        write_record(&project, &external);
        activate(editor, &project);
        let observed = editor.source_tool_selection_view();
        assert_eq!(observed.desired, loaded.desired);
        let probe_attempt = editor.source_tool_view().attempt;

        let attempt = editor
            .disable_installed_source_tool(&observed.basis)
            .unwrap();
        assert!(editor.source_tool_view().active.is_none());
        assert!(!editor.source_tool_view().probing);
        assert!(editor.source_tool_selection_view().active_origin.is_none());
        wait(editor, |editor| {
            let view = editor.source_tool_selection_view();
            view.phase == Phase::Idle
                && view
                    .disable
                    .as_ref()
                    .is_some_and(|disable| disable.attempt == attempt && disable.outcome.is_some())
        });
        let failed = editor.source_tool_selection_view();
        assert_eq!(failed.phase, Phase::Idle);
        assert_eq!(failed.desired, loaded.desired);
        assert_eq!(failed.error, Some(Error::Changed));
        assert_eq!(failed.operation, observed.operation);
        assert_eq!(
            failed.disable,
            Some(EditorSourceToolSelectionAttempt {
                attempt,
                outcome: Some(Outcome::Failed(Error::Changed)),
            }),
        );
        wait(editor, |editor| !editor.source_tool_view().retiring);
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(editor.source_tool_selection_view(), failed);
        let tool = editor.source_tool_view();
        assert!(tool.active.is_none());
        assert!(!tool.probing);
        assert_eq!(tool.attempt, probe_attempt);
        assert_eq!(fs::read(project.root.join(RECORD_PATH)).unwrap(), external);
        assert_eq!(snapshot(editor), before);
        assert_eq!(project.persisted(), persisted);
    });
}
