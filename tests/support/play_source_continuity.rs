use super::*;

use nara::{
    scene::SceneAuthoringRevision,
    tooling::{
        EditorDocumentId, EditorDocumentKind, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentResult,
    },
};

fn open_source_prefab(
    editor: &mut EditorProjectSession,
    source: EditorDocumentId,
) -> EditorDocumentId {
    let revision = editor
        .current_frame()
        .workspace()
        .scene(source)
        .unwrap()
        .revision;
    let anchor = SceneEntityId::new("enemy-anchor").unwrap();
    let outcomes = editor_driver::dispatch(
        editor,
        [
            EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: source }),
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: source,
                entity: Some(anchor.clone()),
            }),
            EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                document: source,
                base_revision: revision,
                entity: anchor,
            }),
        ],
    );
    for outcome in &outcomes {
        let EditorActionOutcome::Workspace(report) = outcome else {
            panic!("expected exact source navigation, got {outcome:?}");
        };
        assert!(report.applied, "{:?}", report.diagnostics);
    }
    let EditorActionOutcome::Workspace(report) = &outcomes[2] else {
        unreachable!();
    };
    let prefab = report
        .opened_document
        .expect("the source Prefab opens a new slot");
    let frame = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_ne!(prefab, source);
    assert_eq!(frame.workspace().active_document, Some(prefab));
    assert_eq!(
        frame.workspace().document(prefab).unwrap().kind,
        EditorDocumentKind::Prefab
    );
    assert!(frame.workspace().scene(source).is_some());
    prefab
}

fn assert_play_source(
    editor: &EditorProjectSession,
    source: EditorDocumentId,
    revision: SceneAuthoringRevision,
) {
    let frame = editor.current_frame();
    assert_eq!(frame.workspace().scene(source).unwrap().revision, revision);
    assert_eq!(frame.play().source_revision(), Some(revision));
    assert_eq!(frame.play().current_revision(), Some(revision));
}

fn close_unrelated_prefab(
    editor: &mut EditorProjectSession,
    source: EditorDocumentId,
    prefab: EditorDocumentId,
) {
    let play = editor.current_frame().play().clone();
    let intent = EditorWorkspaceIntent::CloseDocument { document: prefab };
    assert_eq!(
        editor_driver::request_workspace_intent(editor, intent),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let frame = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert!(frame.workspace().document(prefab).is_none());
    assert!(frame.workspace().scene(source).is_some());
    assert_eq!(
        frame.play(),
        &play,
        "unrelated Close must not consume the Play owner"
    );
    assert_eq!(frame.workspace_intent().intent(), None);
    assert_eq!(
        frame.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied { intent })
    );
    assert!(editor_driver::acknowledge_workspace_intent_result(editor));
    let frame = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_eq!(frame.workspace_intent().result(), None);
    assert_eq!(frame.play(), &play);
}

fn assert_source_close_waits(editor: &mut EditorProjectSession, source: EditorDocumentId) {
    let intent = EditorWorkspaceIntent::CloseDocument { document: source };
    assert_eq!(
        editor_driver::request_workspace_intent(editor, intent),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let frame = editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert!(frame.workspace().scene(source).is_some());
    assert_eq!(frame.workspace_intent().intent(), Some(intent));
    assert_eq!(
        frame.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::RetiringRuntime)
    );
    assert_eq!(frame.workspace_intent().result(), None);
}

#[test]
fn cancelling_unconsumed_start_keeps_its_source_when_an_active_prefab_closes() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.writable_root_capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let frame = editor.current_frame();
    let source = frame.workspace().active_document.unwrap();
    let revision = frame.workspace().scene(source).unwrap().revision;
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Starting
    );

    let prefab = open_source_prefab(&mut editor, source);
    assert_play_source(&editor, source, revision);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Starting
    );
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Cancel),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_eq!(
        editor.current_frame().workspace().active_document,
        Some(prefab)
    );
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::RetiringPlay
    );
    assert_eq!(editor.current_frame().play().generation(), None);
    assert_eq!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Pending {
            operation: EditorPlayOperation::Cancel,
        })
    );
    assert_play_source(&editor, source, revision);
    close_unrelated_prefab(&mut editor, source, prefab);
    assert_play_source(&editor, source, revision);

    assert_source_close_waits(&mut editor, source);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::RetiringPlay
    );
    assert_play_source(&editor, source, revision);
    editor.drive_editor_frame(Duration::ZERO);
    let frame = editor.current_frame();
    assert_eq!(frame.play().state(), EditorPlayState::Empty);
    assert_eq!(frame.play().generation(), None);
    assert_eq!(frame.play().source_revision(), None);
    assert_eq!(
        frame.play().result(),
        Some(EditorPlayOperationResult::Cancelled {
            operation: EditorPlayOperation::Play,
        })
    );
    assert!(frame.workspace().scene(source).is_none());
    assert_eq!(frame.workspace_intent().intent(), None);
    assert_eq!(
        frame.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document: source },
        })
    );
}

#[test]
fn failed_start_keeps_its_source_and_failure_through_retirement_and_prefab_close() {
    for incomplete in [false, true] {
        let project = TestProject::with_prefab_startup();
        project.select_local_headless_profile();
        let release = Arc::new(AtomicBool::new(false));
        let recipe = ProductRecipe::new()
            .add_plugin::<BuildFailurePlugin>()
            .unwrap();
        let intent = TrustedCompiledEditorProjectIntent::new().with_recipe(recipe);
        let intent = if incomplete {
            intent.with_cleanup_timeout(Duration::ZERO)
        } else {
            intent
        };
        let mut editor = open_trusted_compiled_editor_with_probe(
            project.writable_root_capability(),
            intent,
            RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
        )
        .unwrap();
        let frame = editor.current_frame();
        let source = frame.workspace().active_document.unwrap();
        let revision = frame.workspace().scene(source).unwrap().revision;
        assert_eq!(
            editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.current_frame().play().state(),
            EditorPlayState::Starting
        );
        let prefab = open_source_prefab(&mut editor, source);
        assert_play_source(&editor, source, revision);
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.current_frame().play().state(),
            EditorPlayState::RetiringPlay
        );
        assert_play_source(&editor, source, revision);
        let state = if incomplete {
            editor.drive_editor_frame(Duration::ZERO);
            EditorPlayState::RetirementIncomplete
        } else {
            EditorPlayState::RetiringPlay
        };
        assert_eq!(editor.current_frame().play().state(), state);
        assert_eq!(
            editor.current_frame().workspace().active_document,
            Some(prefab)
        );
        assert_eq!(editor.current_frame().play().generation(), None);
        assert_eq!(
            editor.current_frame().play().result(),
            Some(EditorPlayOperationResult::Pending {
                operation: EditorPlayOperation::Play,
            })
        );
        assert_play_source(&editor, source, revision);
        assert_editor_plugin_domain(
            editor.current_frame().diagnostics(),
            "demo.editor.build-unavailable",
            BUILD_FAILURE_PLUGIN_ID,
            "build",
            "project.run.plugin-hook-failed",
        );
        close_unrelated_prefab(&mut editor, source, prefab);
        let retained_prefab = open_source_prefab(&mut editor, source);
        assert_play_source(&editor, source, revision);
        assert_source_close_waits(&mut editor, source);
        if incomplete {
            assert_eq!(
                editor_driver::request_play(&mut editor, EditorPlayCommand::RetryRetirement),
                EditorPlayRequestResult::Accepted
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(editor.current_frame().play().state(), state);
        assert_eq!(
            editor.current_frame().workspace().active_document,
            Some(retained_prefab)
        );
        assert_eq!(
            editor.current_frame().workspace_intent().intent(),
            Some(EditorWorkspaceIntent::CloseDocument { document: source })
        );
        assert_eq!(editor.current_frame().workspace_intent().result(), None);
        assert_eq!(
            editor.current_frame().play().result(),
            Some(EditorPlayOperationResult::Pending {
                operation: if incomplete {
                    EditorPlayOperation::RetryRetirement
                } else {
                    EditorPlayOperation::Play
                },
            })
        );
        assert_play_source(&editor, source, revision);

        release.store(true, Ordering::Release);
        drive_retryable_close_until_empty(&mut editor, 32);
        let frame = editor.current_frame();
        assert_eq!(frame.play().generation(), None);
        assert_eq!(frame.play().source_revision(), None);
        assert_eq!(
            frame.play().result(),
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::Play,
                failure: EditorPlayFailure::Start,
            })
        );
        assert!(frame.workspace().scene(source).is_none());
        assert_eq!(frame.workspace().active_document, Some(retained_prefab));
        assert_eq!(frame.workspace_intent().intent(), None);
        assert_eq!(
            frame.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: source },
            })
        );
        assert_editor_plugin_domain(
            frame.diagnostics(),
            "demo.editor.build-unavailable",
            BUILD_FAILURE_PLUGIN_ID,
            "build",
            "project.run.plugin-hook-failed",
        );
    }
}

#[cfg(all(
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[test]
fn withheld_game_target_keeps_exact_source_and_activation_across_prefab_close() {
    for prepare_activation in [false, true] {
        let project = TestProject::with_prefab_startup();
        let manifest_path = project.path().join("nara.toml");
        let mut manifest = std::fs::read_to_string(&manifest_path).unwrap();
        manifest.push_str(
            r#"
[profiles.desktop.capabilities]
requested = ["runtime-2d", "presentation-2d", "desktop-winit", "render-wgpu"]

[profiles.desktop.runtime]
preset = "local-headless"

[profiles.desktop.window]
enabled = true
width = 960
height = 540
resizable = true
"#,
        );
        std::fs::write(manifest_path, manifest).unwrap();
        let release = Arc::new(AtomicBool::new(false));
        let intent = TrustedCompiledEditorProjectIntent::new()
            .with_profile("desktop")
            .with_cleanup_timeout(Duration::ZERO)
            .with_recipe(
                ProductRecipe::new()
                    .add_plugin::<GameViewCameraPlugin>()
                    .unwrap(),
            );
        let mut editor = open_trusted_compiled_editor_with_probe(
            project.writable_root_capability(),
            intent,
            RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
        )
        .unwrap();
        let frame = editor.current_frame();
        let source = frame.workspace().active_document.unwrap();
        let revision = frame.workspace().scene(source).unwrap().revision;
        assert_eq!(
            editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        editor.drive_editor_frame(Duration::ZERO);
        editor.drive_editor_frame(Duration::ZERO);
        let request = editor
            .pending_game_start_request()
            .expect("the Runtime awaits its Game target");
        assert_eq!(request.document(), source);
        assert_eq!(request.source_revision(), revision);
        let activation = if prepare_activation {
            Some(editor.prepare_game_start_activation(request).unwrap())
        } else {
            None
        };
        let prefab = open_source_prefab(&mut editor, source);
        assert_eq!(
            editor.current_frame().play().state(),
            EditorPlayState::Starting
        );
        assert_eq!(editor.current_frame().play().generation(), None);
        assert_eq!(
            editor.current_frame().play().result(),
            Some(EditorPlayOperationResult::Pending {
                operation: EditorPlayOperation::Play,
            })
        );
        assert_play_source(&editor, source, revision);
        let pending = if prepare_activation {
            None
        } else {
            Some(request)
        };
        assert_eq!(editor.pending_game_start_request(), pending);
        close_unrelated_prefab(&mut editor, source, prefab);
        assert_eq!(editor.pending_game_start_request(), pending);
        if let Some(activation) = &activation {
            assert_eq!(activation.request(), request);
        }
        assert_eq!(editor.active_game_start_request(), None);
        let retained_prefab = open_source_prefab(&mut editor, source);
        assert_source_close_waits(&mut editor, source);
        assert_play_source(&editor, source, revision);

        let (retry, terminal) = if let Some(activation) = activation {
            assert_eq!(
                editor.current_frame().play().state(),
                EditorPlayState::Starting
            );
            assert_eq!(editor.pending_game_start_request(), None);
            assert_eq!(activation.request(), request);
            editor.commit_game_start_activation(activation);
            assert_eq!(editor.active_game_start_request(), Some(request));
            editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
            assert_eq!(
                editor.current_frame().play().state(),
                EditorPlayState::Stopping
            );
            assert_eq!(
                editor.current_frame().play().generation(),
                Some(request.runtime_generation().get())
            );
            assert_play_source(&editor, source, revision);
            drive_until(&mut editor, EditorPlayState::CloseIncomplete, 32);
            assert_eq!(
                editor.current_frame().play().result(),
                Some(EditorPlayOperationResult::Failed {
                    operation: EditorPlayOperation::Stop,
                    failure: EditorPlayFailure::Close,
                })
            );
            (
                EditorPlayCommand::RetryClose,
                EditorPlayOperationResult::Applied {
                    operation: EditorPlayOperation::RetryClose,
                    generation: Some(request.runtime_generation().get()),
                },
            )
        } else {
            assert_eq!(
                editor.current_frame().play().state(),
                EditorPlayState::RetiringPlay
            );
            assert_eq!(editor.pending_game_start_request(), None);
            assert!(matches!(
                editor.prepare_game_start_activation(request),
                Err(EditorGameViewBridgeFailure::GameStartNotPending)
            ));
            drive_until(&mut editor, EditorPlayState::RetirementIncomplete, 32);
            (
                EditorPlayCommand::RetryRetirement,
                EditorPlayOperationResult::Cancelled {
                    operation: EditorPlayOperation::Play,
                },
            )
        };
        assert_eq!(
            editor.current_frame().workspace().active_document,
            Some(retained_prefab)
        );
        assert_play_source(&editor, source, revision);
        assert_eq!(
            editor.current_frame().workspace_intent().intent(),
            Some(EditorWorkspaceIntent::CloseDocument { document: source })
        );
        assert_eq!(editor.current_frame().workspace_intent().result(), None);
        release.store(true, Ordering::Release);
        assert_eq!(
            editor_driver::request_play(&mut editor, retry),
            EditorPlayRequestResult::Accepted
        );
        editor.drive_editor_frame(Duration::ZERO);
        drive_retryable_close_until_empty(&mut editor, 32);
        let frame = editor.current_frame();
        assert_eq!(frame.play().result(), Some(terminal));
        assert_eq!(frame.play().generation(), None);
        assert_eq!(frame.play().source_revision(), None);
        assert!(frame.workspace().scene(source).is_none());
        assert!(frame.workspace().document(retained_prefab).is_some());
        assert_eq!(frame.workspace_intent().intent(), None);
        assert_eq!(
            frame.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: source },
            })
        );
        assert_eq!(editor.pending_game_start_request(), None);
        assert_eq!(editor.active_game_start_request(), None);
    }
}

#[test]
fn stopping_source_keeps_generation_and_close_failure_when_an_active_prefab_closes() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let intent = TrustedCompiledEditorProjectIntent::new()
        .with_cleanup_timeout(Duration::ZERO)
        .with_recipe(
            ProductRecipe::new()
                .add_plugin::<ShutdownFailurePlugin>()
                .unwrap(),
        );
    let mut editor = open_trusted_compiled_editor_with_probe(
        project.writable_root_capability(),
        intent,
        RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
    )
    .unwrap();
    let frame = editor.current_frame();
    let source = frame.workspace().active_document.unwrap();
    let revision = frame.workspace().scene(source).unwrap().revision;
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let generation = editor.current_frame().play().generation().unwrap();
    let prefab = open_source_prefab(&mut editor, source);
    assert_play_source(&editor, source, revision);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame_with_runtime(Duration::ZERO, false);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Stopping
    );
    assert_eq!(editor.current_frame().play().generation(), Some(generation));
    assert_eq!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Pending {
            operation: EditorPlayOperation::Stop,
        })
    );
    assert_play_source(&editor, source, revision);
    close_unrelated_prefab(&mut editor, source, prefab);
    let prefab = open_source_prefab(&mut editor, source);
    drive_until(&mut editor, EditorPlayState::CloseIncomplete, 32);
    let failure = EditorPlayOperationResult::Failed {
        operation: EditorPlayOperation::Stop,
        failure: EditorPlayFailure::Close,
    };
    assert_eq!(editor.current_frame().play().generation(), Some(generation));
    assert_eq!(editor.current_frame().play().result(), Some(failure));
    assert_eq!(
        editor.current_frame().workspace().active_document,
        Some(prefab)
    );
    assert_play_source(&editor, source, revision);
    close_unrelated_prefab(&mut editor, source, prefab);
    let retained_prefab = open_source_prefab(&mut editor, source);
    assert_source_close_waits(&mut editor, source);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::CloseIncomplete
    );
    assert_eq!(editor.current_frame().play().generation(), Some(generation));
    assert_eq!(editor.current_frame().play().result(), Some(failure));
    assert_eq!(editor.current_frame().workspace_intent().result(), None);
    assert_play_source(&editor, source, revision);

    release.store(true, Ordering::Release);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::RetryClose),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    drive_retryable_close_until_empty(&mut editor, 32);
    let frame = editor.current_frame();
    assert_eq!(frame.play().generation(), None);
    assert_eq!(frame.play().source_revision(), None);
    assert_eq!(
        frame.play().result(),
        Some(EditorPlayOperationResult::Failed {
            operation: EditorPlayOperation::RetryClose,
            failure: EditorPlayFailure::Close,
        })
    );
    assert!(frame.workspace().scene(source).is_none());
    assert_eq!(frame.workspace().active_document, Some(retained_prefab));
    assert_eq!(frame.workspace_intent().intent(), None);
    assert_eq!(
        frame.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document: source },
        })
    );
    assert_editor_plugin_domain(
        frame.diagnostics(),
        "demo.editor.shutdown-unavailable",
        SHUTDOWN_FAILURE_PLUGIN_ID,
        "shutdown",
        "project.run.cleanup-plugin-shutdown",
    );
}
