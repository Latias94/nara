#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use dear_imgui_rs::{Context, Key, sys};
use nara::{
    asset::AssetRef,
    reflect::{ComponentFieldId, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocumentCandidate, SceneDocumentCandidate, SceneEntityId, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentKind, EditorDocumentMutation,
        EditorExternalPlayCompilerLevel, EditorExternalPlayMessageKind, EditorExternalPlayState,
        EditorFrameModel, EditorImageColorSpace, EditorImageImportAttemptView,
        EditorImageImportAvailabilityView, EditorImageImportFreshness,
        EditorImageImportMetadataView, EditorImageImportSettings, EditorNavigationAction,
        EditorPersistenceAction, EditorPersistenceResult, EditorPlayAction, EditorPlayOperation,
        EditorPlayOperationResult, EditorReopenTarget, EditorRuntimeAvailability,
        PrefabCompositionCommand, PrefabCompositionEntityDraft, SceneCompositionCommand,
        SceneCompositionEntityKind, SceneInspectorCommand, SceneInspectorComponentView,
    },
};
use nara_editor::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, ImguiEditorFrontend,
    workspace::EditorPanelTarget,
};
use serde_json::Value;

#[path = "support/imgui.rs"]
mod imgui;
#[path = "support/rust_play_iteration_measurement.rs"]
mod rust_play_iteration_measurement;

const CAMERA: &str = "nara.render.Camera2d";
const COLLECTIBLE: &str = "collect-game.Collectible";
const PLAYER_CONFIG: &str = "collect-game.PlayerConfig";
const SPRITE: &str = "nara.sprite.Sprite";
const IMAGE_SOURCE: &str = "assets/textures/token.png";
const COMPILER_ERROR: &str = "first-game workflow injected compiler error";

#[test]
fn ordinary_browser_keyboard_filter_selects_existing_image() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/generic-editor/image-import");
    let application = open_application(&root);
    let mut application =
        select_browser_image(application, "ready.png", "assets/textures/ready.png");
    assert!(
        application
            .current_frame()
            .unwrap()
            .image_import()
            .is_some()
    );
    close_application(&mut application);
}

/// This action-driven test opens real native game children, but does not drive gameplay input.
#[test]
#[ignore = "set NARA_AUTHOR_WORKFLOW_PROJECT to a disposable external collect-game project; runs nested Cargo and native windows"]
fn independent_game_edits_reopens_plays_and_recovers_from_compile_failure() {
    let root = PathBuf::from(std::env::var_os("NARA_AUTHOR_WORKFLOW_PROJECT").expect(
        "set NARA_AUTHOR_WORKFLOW_PROJECT to a disposable project created outside the engine checkout",
    ))
    .canonicalize()
    .expect("the explicitly supplied disposable project must exist");
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    assert!(
        !root.starts_with(&checkout),
        "use an external disposable project, not the maintained sample"
    );
    let scene_path = root.join("scenes/startup.scene.json");
    let original_source = fs::read(&scene_path).unwrap();
    let original_collectible = collectible_record(&scene_path, "gem-left");
    let baseline_player_x = run_headless(&root);
    assert_eq!(
        baseline_player_x, 108.0,
        "use a fresh external Collect Game"
    );
    prove_external_creation_composition(&root, baseline_player_x);
    let mut application = open_application(&root);
    assert_eq!(
        current_frame(&application).runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    );
    let unavailable = application
        .active_authoring_unavailable_record_manifest()
        .unwrap();
    assert_eq!(unavailable.total_count(), 0);
    edit_saved_player_prefab(&mut application, &root, baseline_player_x);
    select(&mut application, "gem-right");
    assert!(
        selected_component(&current_frame(&application), COLLECTIBLE)
            .access
            .is_editable(),
        "the ordinary Editor must support the exact compiled scalar schema"
    );
    let original_radius = collectible_radius(&current_frame(&application));
    assert_eq!(
        original_radius, 14.0,
        "use a fresh copy of the maintained sample"
    );
    let edited_radius = 30.5;
    edit_collectible_radius(&mut application, edited_radius);
    assert_eq!(
        collectible_radius(&current_frame(&application)),
        edited_radius
    );
    let accepted = current_frame(&application);
    let accepted_scene = accepted.workspace().active_scene.as_ref().unwrap();
    edit_collectible_radius(&mut application, f64::MAX);
    let rejected = current_frame(&application);
    let rejected_scene = rejected.workspace().active_scene.as_ref().unwrap();
    assert_eq!(rejected_scene.revision, accepted_scene.revision);
    assert_eq!(rejected_scene.dirty, accepted_scene.dirty);
    assert_eq!(collectible_radius(&rejected), edited_radius);
    assert_eq!(fs::read(&scene_path).unwrap(), original_source);
    let undone = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document: rejected_scene.document,
            base_revision: rejected_scene.revision,
        }),
    );
    assert_eq!(collectible_radius(&undone), original_radius);
    let scene = undone.workspace().active_scene.as_ref().unwrap();
    let redone = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Redo {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
    assert_eq!(collectible_radius(&redone), edited_radius);
    application = reimport_and_assign_image(application, &root);
    let assigned_image = sprite_image(&current_frame(&application)).clone();
    select(&mut application, "main-camera");
    let opened = current_frame(&application);
    let component = selected_component(&opened, CAMERA);
    assert!(component.access.is_editable());
    let original_height = camera_height(&opened);
    let edited_height = original_height + 24.0;
    let scene = opened.workspace().active_scene.as_ref().unwrap();
    let editing = Instant::now();
    let edited = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            document: scene.document,
            base_revision: scene.revision,
            command: SceneInspectorCommand::SetField {
                entity: SceneEntityId::new("main-camera").unwrap(),
                component: ComponentTypeId::new(CAMERA),
                component_version: component.schema_version,
                field: ComponentFieldId::new("viewport_height"),
                value: ComponentValue::f64(edited_height).unwrap(),
            },
        }),
    );
    assert_eq!(camera_height(&edited), edited_height);
    let scene = edited.workspace().active_scene.as_ref().unwrap();
    let undone = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
    assert_eq!(camera_height(&undone), original_height);
    let scene = undone.workspace().active_scene.as_ref().unwrap();
    let redone = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Redo {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
    assert_eq!(camera_height(&redone), edited_height);
    let scene = redone.workspace().active_scene.as_ref().unwrap();
    let saved = dispatch(
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
    assert!(
        matches!(saved.persistence().result(), Some(EditorPersistenceResult::Saved {
        document, revision, ..
    }) if document == scene.document && revision == scene.revision)
    );
    assert!(!saved.workspace().active_scene.as_ref().unwrap().dirty);
    assert_eq!(
        collectible_record(&scene_path, "gem-left"),
        original_collectible
    );
    assert_eq!(
        collectible_record(&scene_path, "gem-right")["value"]["value"]["radius"]["value"],
        edited_radius
    );
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(unavailable.clone())
    );
    let acknowledged = dispatch(
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
            attempt: saved
                .persistence()
                .attempt()
                .expect("the observed persistence result is terminal"),
        }),
    );
    assert!(acknowledged.persistence().result().is_none());
    close_application(&mut application);

    let mut application = open_application(&root);
    select(&mut application, "gem-right");
    assert_eq!(
        collectible_radius(&current_frame(&application)),
        edited_radius
    );
    assert!(
        selected_component(&current_frame(&application), COLLECTIBLE)
            .access
            .is_editable()
    );
    select(&mut application, "gem-left");
    assert_eq!(sprite_image(&current_frame(&application)), &assigned_image);
    select(&mut application, "main-camera");
    assert_eq!(camera_height(&current_frame(&application)), edited_height);
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(unavailable)
    );
    assert_eq!(
        collectible_record(&scene_path, "gem-left"),
        original_collectible
    );
    eprintln!(
        "custom radius and camera edit/undo/redo/save/reopen: {:?}",
        editing.elapsed()
    );
    let edited_player_x = run_headless(&root);
    assert!(
        edited_player_x < baseline_player_x,
        "the saved larger radius must make the compiled game win earlier: before={baseline_player_x}, after={edited_player_x}"
    );
    eprintln!(
        "saved custom radius reached strict gameplay: player.x {baseline_player_x} -> {edited_player_x}"
    );

    let playing = Instant::now();
    play(&mut application);
    let running = wait_external(
        &mut application,
        EditorExternalPlayState::Running,
        EditorPlayOperation::Play,
    );
    let first_generation = running.play().external().unwrap().generation().unwrap();
    eprintln!(
        "first supervised Play through READY: {:?}",
        playing.elapsed()
    );
    stop(&mut application);

    let mut source = SourceRestore::new(root.join("src/bin/desktop.rs"));
    let mut broken = format!("compile_error!(\"{COMPILER_ERROR}\");\n").into_bytes();
    broken.extend_from_slice(&source.original);
    fs::write(&source.path, broken).unwrap();
    let failing = Instant::now();
    play(&mut application);
    let faulted = wait_external(
        &mut application,
        EditorExternalPlayState::Faulted,
        EditorPlayOperation::Play,
    );
    let external = faulted.play().external().unwrap();
    assert!(external.diagnostics().has_errors());
    assert!(
        external.output().messages().iter().any(|message| {
            message.kind()
                == EditorExternalPlayMessageKind::Compiler(EditorExternalPlayCompilerLevel::Error)
                && message.text().contains(COMPILER_ERROR)
        }),
        "the actual compiler failure must reach the ordinary Editor output"
    );
    assert!(external.last_good_available());
    assert!(external.generation().is_none());
    select(&mut application, "gem-right");
    assert!(
        selected_component(&current_frame(&application), COLLECTIBLE)
            .access
            .is_editable()
    );
    assert_eq!(
        collectible_radius(&current_frame(&application)),
        edited_radius
    );
    eprintln!(
        "deliberate compiler failure through diagnostics: {:?}",
        failing.elapsed()
    );
    source.restore();
    stop(&mut application);

    let recovering = Instant::now();
    play(&mut application);
    let recovered = wait_external(
        &mut application,
        EditorExternalPlayState::Running,
        EditorPlayOperation::Play,
    );
    assert_ne!(
        recovered.play().external().unwrap().generation(),
        Some(first_generation)
    );
    eprintln!(
        "repaired supervised Play through READY: {:?}",
        recovering.elapsed()
    );
    stop(&mut application);
    close_application(&mut application);
    assert_eq!(
        collectible_record(&scene_path, "gem-left"),
        original_collectible
    );
}

fn prove_external_creation_composition(root: &Path, baseline_player_x: f64) {
    let mut scene_source = SourceRestore::new(root.join("scenes/startup.scene.json"));
    let mut prefab_source = SourceRestore::new(root.join("prefabs/player.prefab.json"));
    let mut application = open_application(root);
    for (kind, source) in [
        (EditorDocumentKind::Scene, &scene_source),
        (EditorDocumentKind::Prefab, &prefab_source),
    ] {
        if kind == EditorDocumentKind::Prefab {
            select(&mut application, "player-anchor");
            let frame = current_frame(&application);
            let scene = frame.workspace().active_scene.as_ref().unwrap();
            dispatch(
                &mut application,
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: SceneEntityId::new("player-anchor").unwrap(),
                }),
            );
        }
        let original: Value = serde_json::from_slice(&source.original).unwrap();
        let frame = current_frame(&application);
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        assert_eq!(scene.kind, kind);
        assert!(scene.editor.inspector.composition.generic_authoring);
        assert!(!scene.dirty);
        assert_eq!(scene.editor.inspector.history.undo_depth, 0);
        let document = scene.document;
        let unavailable = application
            .active_authoring_unavailable_record_manifest()
            .unwrap();
        let placement = ScenePlacement::last(ScenePlacementOwner::Root);
        let create = match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement,
                },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document,
                base_revision: scene.revision,
                command: PrefabCompositionCommand::CreateEntity {
                    draft: PrefabCompositionEntityDraft::new(Default::default()).unwrap(),
                    placement,
                },
            },
            EditorDocumentKind::Extension(_) => {
                unreachable!("this consumer exercises Scene and Prefab documents")
            }
        };
        let created = dispatch(&mut application, EditorAction::Mutate(create));
        let scene = created.workspace().active_scene.as_ref().unwrap();
        let entity = scene.selection.top_entity().unwrap().clone();
        assert_eq!(scene.editor.inspector.history.undo_depth, 1);
        assert!(
            scene
                .editor
                .inspector
                .selected_entity_view
                .as_ref()
                .unwrap()
                .components
                .is_empty()
        );
        assert!(
            scene
                .editor
                .inspector
                .composition
                .components
                .iter()
                .any(|component| {
                    component.component.as_str() == COLLECTIBLE
                        && component.add_unavailable_reason.is_none()
                })
        );
        assert_eq!(
            application.active_authoring_unavailable_record_manifest(),
            Some(unavailable.clone())
        );
        let add = match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::AddComponent {
                    entity: entity.clone(),
                    component: ComponentTypeId::new(COLLECTIBLE),
                },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document,
                base_revision: scene.revision,
                command: PrefabCompositionCommand::AddComponent {
                    entity: entity.clone(),
                    component: ComponentTypeId::new(COLLECTIBLE),
                },
            },
            EditorDocumentKind::Extension(_) => {
                unreachable!("this consumer exercises Scene and Prefab documents")
            }
        };
        let added = dispatch(&mut application, EditorAction::Mutate(add));
        assert_eq!(collectible_radius(&added), 20.0);
        let component = selected_component(&added, COLLECTIBLE);
        assert_eq!(component.document_version.0, 2);
        assert_eq!(component.fields.len(), 1);
        let scene = added.workspace().active_scene.as_ref().unwrap();
        assert_eq!(scene.editor.inspector.history.undo_depth, 2);
        assert_eq!(
            application.active_authoring_unavailable_record_manifest(),
            Some(unavailable.clone())
        );
        let edited = dispatch(
            &mut application,
            EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision: scene.revision,
                command: SceneInspectorCommand::SetField {
                    entity: entity.clone(),
                    component: ComponentTypeId::new(COLLECTIBLE),
                    component_version: component.schema_version,
                    field: ComponentFieldId::new("radius"),
                    value: ComponentValue::f64(23.5).unwrap(),
                },
            }),
        );
        assert_eq!(collectible_radius(&edited), 23.5);
        assert_eq!(
            application.active_authoring_unavailable_record_manifest(),
            Some(unavailable.clone())
        );
        assert_eq!(
            edited
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .history
                .undo_depth,
            3
        );
        for (redo, radius, depth) in [(false, 20.0, 2), (true, 23.5, 3)] {
            let frame = current_frame(&application);
            let scene = frame.workspace().active_scene.as_ref().unwrap();
            let command = if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision: scene.revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision: scene.revision,
                }
            };
            let frame = dispatch(&mut application, EditorAction::Mutate(command));
            assert_eq!(collectible_radius(&frame), radius);
            assert_eq!(
                frame
                    .workspace()
                    .active_scene
                    .as_ref()
                    .unwrap()
                    .editor
                    .inspector
                    .history
                    .undo_depth,
                depth
            );
            assert_eq!(
                application.active_authoring_unavailable_record_manifest(),
                Some(unavailable.clone())
            );
        }
        let frame = current_frame(&application);
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let remove = match kind {
            EditorDocumentKind::Scene => EditorDocumentMutation::ComposeScene {
                document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::RemoveComponent {
                    entity: entity.clone(),
                    component: ComponentTypeId::new(COLLECTIBLE),
                },
            },
            EditorDocumentKind::Prefab => EditorDocumentMutation::ComposePrefab {
                document,
                base_revision: scene.revision,
                command: PrefabCompositionCommand::RemoveComponent {
                    entity: entity.clone(),
                    component: ComponentTypeId::new(COLLECTIBLE),
                },
            },
            EditorDocumentKind::Extension(_) => {
                unreachable!("this consumer exercises Scene and Prefab documents")
            }
        };
        dispatch(&mut application, EditorAction::Mutate(remove));
        for (redo, depth) in [
            (None, 4),
            (Some(false), 3),
            (Some(true), 4),
            (Some(false), 3),
        ] {
            if let Some(redo) = redo {
                let frame = current_frame(&application);
                let scene = frame.workspace().active_scene.as_ref().unwrap();
                let command = if redo {
                    EditorDocumentMutation::Redo {
                        document,
                        base_revision: scene.revision,
                    }
                } else {
                    EditorDocumentMutation::Undo {
                        document,
                        base_revision: scene.revision,
                    }
                };
                dispatch(&mut application, EditorAction::Mutate(command));
            }
            let frame = current_frame(&application);
            let scene = frame.workspace().active_scene.as_ref().unwrap();
            assert_eq!(scene.selection.top_entity(), Some(&entity));
            assert_eq!(scene.editor.inspector.history.undo_depth, depth);
            let components = &scene
                .editor
                .inspector
                .selected_entity_view
                .as_ref()
                .unwrap()
                .components;
            if depth == 4 {
                assert!(components.is_empty());
            } else {
                assert_eq!(components.len(), 1);
                assert_eq!(collectible_radius(&frame), 23.5);
            }
            assert_eq!(
                application.active_authoring_unavailable_record_manifest(),
                Some(unavailable.clone())
            );
        }
        let frame = current_frame(&application);
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        assert!(scene.dirty);
        let saved = dispatch(
            &mut application,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: scene.revision,
            }),
        );
        let scene = saved.workspace().active_scene.as_ref().unwrap();
        let saved_result = saved.persistence().result();
        let saved_exact_revision = matches!(
            saved_result,
            Some(EditorPersistenceResult::Saved { document: actual, revision, .. })
                if actual == document && revision == scene.revision
        );
        if !saved_exact_revision {
            close_application(&mut application);
        }
        assert!(
            saved_exact_revision,
            "the {kind:?} composition Save must persist its exact revision: {saved_result:?}"
        );
        assert!(!scene.dirty);
        assert_eq!(
            application.active_authoring_unavailable_record_manifest(),
            Some(unavailable.clone())
        );
        let mut expected = original;
        expected["payload"]["roots"]
            .as_array_mut()
            .unwrap()
            .push(Value::from(entity.as_str()));
        expected["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": entity.as_str(),
                "children": [],
                "components": {
                    COLLECTIBLE: { "version": 2, "owner": "collect-game", "value": {
                        "type": "map", "value": { "radius": { "type": "f64", "value": 23.5 } }
                    } }
                }
            }));
        // Compare complete source values through the existing canonical file codec, including
        // entity ordering and the durable representation of typed floating-point values.
        let expected_bytes = serde_json::to_vec(&expected).unwrap();
        let expected_json = match kind {
            EditorDocumentKind::Scene => SceneDocumentCandidate::decode_json_bytes(&expected_bytes)
                .unwrap()
                .to_json_string()
                .unwrap(),
            EditorDocumentKind::Prefab => {
                PrefabDocumentCandidate::decode_json_bytes(&expected_bytes)
                    .unwrap()
                    .to_json_string()
                    .unwrap()
            }
            EditorDocumentKind::Extension(_) => {
                unreachable!("this consumer exercises Scene and Prefab documents")
            }
        };
        let expected: Value = serde_json::from_str(&expected_json).unwrap();
        let stored: Value = serde_json::from_slice(&fs::read(&source.path).unwrap()).unwrap();
        if stored != expected {
            close_application(&mut application);
        }
        assert_eq!(
            stored, expected,
            "{kind:?} composition must preserve the complete source outside its new root entity"
        );
        let target = EditorReopenTarget::OpenDocument {
            document,
            base_revision: scene.revision,
        };
        dispatch(
            &mut application,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                attempt: saved
                    .persistence()
                    .attempt()
                    .expect("the observed persistence result is terminal"),
            }),
        );
        let reopened = dispatch(
            &mut application,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
        );
        let scene = reopened.workspace().active_scene.as_ref().unwrap();
        assert_eq!(scene.document, document);
        assert_eq!(scene.kind, kind);
        assert!(!scene.dirty);
        assert_eq!(scene.editor.inspector.history.undo_depth, 0);
        assert_eq!(scene.editor.inspector.history.redo_depth, 0);
        assert!(
            matches!(reopened.persistence().result(), Some(EditorPersistenceResult::Opened { document: actual, revision, .. }) if actual == document && revision == scene.revision)
        );
        select(&mut application, entity.as_str());
        assert_eq!(collectible_radius(&current_frame(&application)), 23.5);
        assert_eq!(
            application.active_authoring_unavailable_record_manifest(),
            Some(unavailable)
        );
        assert_eq!(
            run_headless(root),
            baseline_player_x,
            "the saved {kind:?} root containing only Collectible must be admitted without changing gameplay"
        );
        eprintln!(
            "external {kind:?} create/add/edit/remove/undo/redo/save/reopen: strict player.x={baseline_player_x}"
        );
        let acknowledged = dispatch(
            &mut application,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                attempt: reopened
                    .persistence()
                    .attempt()
                    .expect("the observed persistence result is terminal"),
            }),
        );
        assert!(acknowledged.persistence().result().is_none());
    }
    close_application(&mut application);
    // Restore this disposable fixture after the proof closes; this is not product entity deletion.
    scene_source.restore();
    prefab_source.restore();
    assert_eq!(fs::read(&scene_source.path).unwrap(), scene_source.original);
    assert_eq!(
        fs::read(&prefab_source.path).unwrap(),
        prefab_source.original
    );
}

fn edit_saved_player_prefab(
    application: &mut EditorApplication,
    root: &Path,
    baseline_player_x: f64,
) {
    let scene_path = root.join("scenes/startup.scene.json");
    let original_scene = fs::read(&scene_path).unwrap();
    let prefab_path = root.join("prefabs/player.prefab.json");
    let original_prefab: Value = serde_json::from_slice(&fs::read(&prefab_path).unwrap()).unwrap();
    let unavailable = application
        .active_authoring_unavailable_record_manifest()
        .unwrap();
    select(application, "player-anchor");
    let frame = current_frame(application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert_eq!(scene.kind, EditorDocumentKind::Scene);
    let scene_document = scene.document;
    let opened = dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
            document: scene_document,
            base_revision: scene.revision,
            entity: SceneEntityId::new("player-anchor").unwrap(),
        }),
    );
    let prefab = opened.workspace().active_scene.as_ref().unwrap();
    assert_eq!(prefab.kind, EditorDocumentKind::Prefab);
    let prefab_document = prefab.document;
    assert_ne!(prefab_document, scene_document);
    select(application, "player");
    assert_eq!(
        float_field(&current_frame(application), PLAYER_CONFIG, "speed"),
        200.0
    );

    for (speed, expected_player_x) in [(240.0, 110.4), (200.0, baseline_player_x)] {
        let frame = current_frame(application);
        let prefab = frame.workspace().document(prefab_document).unwrap();
        let component = selected_component(&frame, PLAYER_CONFIG);
        assert!(component.access.is_editable());
        let edited = dispatch(
            application,
            EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                document: prefab_document,
                base_revision: prefab.revision,
                command: SceneInspectorCommand::SetField {
                    entity: SceneEntityId::new("player").unwrap(),
                    component: ComponentTypeId::new(PLAYER_CONFIG),
                    component_version: component.schema_version,
                    field: ComponentFieldId::new("speed"),
                    value: ComponentValue::f64(speed).unwrap(),
                },
            }),
        );
        assert_eq!(float_field(&edited, PLAYER_CONFIG, "speed"), speed);
        assert!(edited.workspace().document(prefab_document).unwrap().dirty);
        if speed == 240.0 {
            let prefab = edited.workspace().document(prefab_document).unwrap();
            let undone = dispatch(
                application,
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document: prefab_document,
                    base_revision: prefab.revision,
                }),
            );
            assert_eq!(float_field(&undone, PLAYER_CONFIG, "speed"), 200.0);
            let prefab = undone.workspace().document(prefab_document).unwrap();
            let redone = dispatch(
                application,
                EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document: prefab_document,
                    base_revision: prefab.revision,
                }),
            );
            assert_eq!(float_field(&redone, PLAYER_CONFIG, "speed"), speed);
        }
        let frame = current_frame(application);
        let prefab = frame.workspace().document(prefab_document).unwrap();
        let saved = dispatch(
            application,
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document: prefab_document,
                base_revision: prefab.revision,
            }),
        );
        assert!(matches!(
            saved.persistence().result(),
            Some(EditorPersistenceResult::Saved { document, revision, .. })
                if document == prefab_document && revision == prefab.revision
        ));
        let prefab = saved.workspace().document(prefab_document).unwrap();
        assert!(!prefab.dirty);
        let target = EditorReopenTarget::OpenDocument {
            document: prefab_document,
            base_revision: prefab.revision,
        };
        dispatch(
            application,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                attempt: saved
                    .persistence()
                    .attempt()
                    .expect("the observed persistence result is terminal"),
            }),
        );
        let reopened = dispatch(
            application,
            EditorAction::Persistence(EditorPersistenceAction::Reopen { target }),
        );
        let prefab = reopened.workspace().document(prefab_document).unwrap();
        assert_eq!(reopened.workspace().active_document, Some(prefab_document));
        assert_eq!(prefab.kind, EditorDocumentKind::Prefab);
        assert!(!prefab.dirty);
        assert!(matches!(
            reopened.persistence().result(),
            Some(EditorPersistenceResult::Opened { document, revision, .. })
                if document == prefab_document && revision == prefab.revision
        ));
        select(application, "player");
        assert_eq!(
            float_field(&current_frame(application), PLAYER_CONFIG, "speed"),
            speed
        );
        assert_eq!(
            application.active_authoring_unavailable_record_manifest(),
            Some(unavailable.clone())
        );
        let mut expected_prefab = original_prefab.clone();
        expected_prefab["payload"]["entities"][0]["components"][PLAYER_CONFIG]["value"]["value"]
            ["speed"]["value"] = Value::from(speed);
        let saved_prefab: Value = serde_json::from_slice(&fs::read(&prefab_path).unwrap()).unwrap();
        assert_eq!(saved_prefab, expected_prefab);
        assert_eq!(fs::read(&scene_path).unwrap(), original_scene);
        let player_x = run_headless(root);
        assert_eq!(
            player_x, expected_player_x,
            "the strict game must use the saved Prefab speed {speed}"
        );
        if speed == 240.0 {
            assert_ne!(player_x, baseline_player_x);
            eprintln!(
                "saved Prefab custom speed reached strict gameplay: player.x {baseline_player_x} -> {player_x}"
            );
        }
        dispatch(
            application,
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                attempt: reopened
                    .persistence()
                    .attempt()
                    .expect("the observed persistence result is terminal"),
            }),
        );
    }
    let restored_prefab: Value = serde_json::from_slice(&fs::read(&prefab_path).unwrap()).unwrap();
    assert_eq!(restored_prefab, original_prefab);
    let restored_scene = dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
            document: scene_document,
        }),
    );
    assert_eq!(
        restored_scene.workspace().active_document,
        Some(scene_document)
    );
    assert!(
        !restored_scene
            .workspace()
            .document(scene_document)
            .unwrap()
            .dirty
    );
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(unavailable)
    );
}

fn run_headless(root: &Path) -> f64 {
    let output = Command::new(env!("CARGO"))
        .args(["run", "--locked", "--quiet", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .args(["--bin", "headless"])
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "1")
        .output()
        .expect("run the disposable project's compiled headless game");
    assert!(
        output.status.success(),
        "strict headless game failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("the game's outcome is UTF-8");
    let outcome = stdout
        .lines()
        .find(|line| line.starts_with("collect_headless: ok "))
        .expect("the compiled game must accept and report its gameplay outcome");
    assert!(outcome.contains("collected=3 targets=3 won=true resets=0"));
    outcome
        .split_once("player=(")
        .and_then(|(_, position)| position.split_once(','))
        .expect("the observable game outcome includes player.x")
        .0
        .parse()
        .expect("the observable player.x is a number")
}

fn edit_collectible_radius(application: &mut EditorApplication, radius: f64) {
    let frame = current_frame(application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    dispatch(
        application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            document: scene.document,
            base_revision: scene.revision,
            command: SceneInspectorCommand::SetField {
                entity: SceneEntityId::new("gem-right").unwrap(),
                component: ComponentTypeId::new(COLLECTIBLE),
                component_version: selected_component(&frame, COLLECTIBLE).schema_version,
                field: ComponentFieldId::new("radius"),
                value: ComponentValue::f64(radius).unwrap(),
            },
        }),
    );
}

fn reimport_and_assign_image(mut application: EditorApplication, root: &Path) -> EditorApplication {
    let pixels = fs::read(root.join(IMAGE_SOURCE)).unwrap();
    let mut metadata = SourceRestore::new(root.join(format!("{IMAGE_SOURCE}.meta")));
    settle_image_work(&mut application);
    let mut application = select_browser_image(application, "token.png", IMAGE_SOURCE);
    let frame = current_frame(&application);
    let detail = frame
        .image_import()
        .expect("the ordinary Project Browser selects the PNG detail");
    let stable_id = match detail.metadata() {
        EditorImageImportMetadataView::Ready {
            stable_id,
            committed_settings,
        } => {
            assert_eq!(committed_settings, EditorImageImportSettings::default());
            stable_id
        }
        metadata => panic!("the sample PNG must retain its sRGB metadata: {metadata:?}"),
    };
    for color_space in [EditorImageColorSpace::Linear, EditorImageColorSpace::Srgb] {
        let frame = current_frame(&application);
        let detail = frame.image_import().unwrap();
        assert!(detail.can_submit());
        let settings = EditorImageImportSettings::new(color_space);
        let submitted = dispatch(
            &mut application,
            EditorAction::ImageImport(detail.submit_action(settings)),
        );
        let attempt = submitted.image_import().unwrap().attempt();
        assert!(
            matches!(
                attempt,
                EditorImageImportAttemptView::Pending { .. }
                    | EditorImageImportAttemptView::Succeeded { .. }
            ),
            "Reimport must be accepted: {attempt:?}"
        );
        settle_image_work(&mut application);
        let frame = current_frame(&application);
        let imported = frame.image_import().unwrap();
        assert!(matches!(
            imported.attempt(),
            EditorImageImportAttemptView::Succeeded { .. }
        ));
        assert_eq!(
            imported.metadata(),
            EditorImageImportMetadataView::Ready {
                stable_id,
                committed_settings: settings
            }
        );
        assert!(
            matches!(imported.availability(), EditorImageImportAvailabilityView::LastGood {
            stable_id: actual, active_settings, freshness: EditorImageImportFreshness::Fresh, ..
        } if actual == stable_id && active_settings == settings)
        );
    }
    metadata.restored = true;
    assert_eq!(fs::read(root.join(IMAGE_SOURCE)).unwrap(), pixels);
    select(&mut application, "gem-left");
    let frame = current_frame(&application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let original = sprite_image(&frame).clone();
    let assigned = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            document: scene.document,
            base_revision: scene.revision,
            command: SceneInspectorCommand::SetAssetRefField {
                entity: SceneEntityId::new("gem-left").unwrap(),
                component: ComponentTypeId::new(SPRITE),
                component_version: selected_component(&frame, SPRITE).schema_version,
                field: ComponentFieldId::new("material.image"),
                asset_ref: AssetRef::StableId(stable_id),
            },
        }),
    );
    let assigned_value = sprite_image(&assigned).clone();
    assert_eq!(
        assigned_value,
        ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_owned())),
            ("value", ComponentValue::String(stable_id.to_string())),
        ])
    );
    assert_ne!(assigned_value, original);
    let scene = assigned.workspace().active_scene.as_ref().unwrap();
    let undone = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
    assert_eq!(sprite_image(&undone), &original);
    let scene = undone.workspace().active_scene.as_ref().unwrap();
    let redone = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::Redo {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
    assert_eq!(sprite_image(&redone), &assigned_value);
    application
}

fn select_browser_image(
    mut application: EditorApplication,
    filter: &str,
    image: &str,
) -> EditorApplication {
    settle_image_work(&mut application);
    let hierarchy = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.hierarchy")
        .unwrap()
        .id();
    application
        .panel_workspace_mut()
        .layout_mut()
        .set_visible(hierarchy, false);
    application
        .panel_workspace_mut()
        .open("nara.project-browser", EditorPanelTarget::ActiveDocument)
        .unwrap();
    let mut context = imgui::context();
    let mut frontend = ImguiEditorFrontend::new();
    for _ in 0..4 {
        draw_browser(&mut context, &mut frontend, &mut application);
    }
    let binding = context.binding();
    let ui = context.frame();
    binding.with_bound_context(|| {
        // SAFETY: all queries and focus changes remain inside this test's live Context. Request
        // navigation after NewFrame clears prior requests and before the real widgets submit.
        unsafe {
            let current = &mut *sys::igGetCurrentContext();
            let window = (0..current.Windows.Size)
                .map(|index| *current.Windows.Data.add(index as usize))
                .find(|window| {
                    std::ffi::CStr::from_ptr((**window).Name)
                        .to_bytes()
                        .starts_with(b"Project###nara-panel-")
                })
                .expect("the registered Project Browser window must exist");
            sys::igFocusWindow(window, 0);
            sys::igNavInitWindow(window, true);
            sys::igSetNavCursorVisible(true);
        }
    });
    let batch = frontend.show(ui, &mut application).unwrap();
    let first_item = binding.with_bound_context(|| {
        // SAFETY: only the copied result of this frame's real navigation request leaves scope.
        unsafe { (*sys::igGetCurrentContext()).NavInitResult.ID }
    });
    assert_ne!(
        first_item, 0,
        "the Project Browser must submit its initial navigation item"
    );
    let _ = context.render_legacy();
    if let Some(batch) = batch {
        frontend.submit_and_drive(&mut application, batch, Duration::ZERO);
    }
    draw_browser(&mut context, &mut frontend, &mut application);
    binding.with_bound_context(|| {
        // SAFETY: this Context owns the just-submitted navigation facts.
        unsafe {
            let current = &*sys::igGetCurrentContext();
            assert_eq!(current.NavId, first_item);
            assert!(current.NavIdIsAlive);
        }
    });
    // Refresh is the first focusable browser control; Tab enters its real Filter sources input.
    context.io_mut().add_key_event(Key::Tab, true);
    draw_browser(&mut context, &mut frontend, &mut application);
    let filter_item = binding.with_bound_context(|| {
        // SAFETY: capture the concrete next item selected by the actual Tab request, without
        // reproducing any private workspace ID prefix.
        unsafe { (*sys::igGetCurrentContext()).NavMoveResultLocal.ID }
    });
    assert_ne!(filter_item, 0, "Tab must locate the browser's next widget");
    assert_ne!(filter_item, first_item);
    context.io_mut().add_key_event(Key::Tab, false);
    draw_browser(&mut context, &mut frontend, &mut application);
    binding.with_bound_context(|| {
        // SAFETY: the live input state must belong to the exact item selected by keyboard
        // navigation. Unlike WantTextInput, these facts describe this completed frame.
        unsafe {
            let current = &*sys::igGetCurrentContext();
            assert_eq!(current.NavId, filter_item);
            assert_eq!(
                current.ActiveId, filter_item,
                "Tab must activate the real Filter input"
            );
            assert_eq!(current.InputTextState.ID, filter_item);
        }
    });
    context.io_mut().add_key_event(Key::ModCtrl, true);
    context.io_mut().add_key_event(Key::A, true);
    draw_browser(&mut context, &mut frontend, &mut application);
    context.io_mut().add_key_event(Key::A, false);
    context.io_mut().add_key_event(Key::ModCtrl, false);
    draw_browser(&mut context, &mut frontend, &mut application);
    context.io_mut().add_input_characters_utf8(filter);
    draw_browser(&mut context, &mut frontend, &mut application);
    assert_eq!(
        current_frame(&application).project_browser().path_filter(),
        filter,
        "the ordinary Project filter must receive text before selecting its PNG row"
    );
    press_browser_key(&mut context, &mut frontend, &mut application, Key::Enter);
    settle_image_work(&mut application);
    for _ in 0..3 {
        draw_browser(&mut context, &mut frontend, &mut application);
    }
    assert!(
        current_frame(&application)
            .project_browser()
            .rows()
            .iter()
            .any(|row| row.relative_path() == image)
    );
    let path = std::ffi::CString::new(image).unwrap();
    let label = std::ffi::CString::new(format!("[AST] {image}")).unwrap();
    let image_id = context.binding().with_bound_context(|| {
        // SAFETY: the rendered child window owns the concrete root ID. Only its copied item ID
        // leaves this scope; Enter below still executes the production Selectable widget.
        unsafe {
            let current = &mut *sys::igGetCurrentContext();
            let window = (0..current.Windows.Size)
                .map(|index| *current.Windows.Data.add(index as usize))
                .find(|window| {
                    let name = std::ffi::CStr::from_ptr((**window).Name).to_string_lossy();
                    name.starts_with("Project###nara-panel-") && name.contains("/project_sources_")
                })
                .expect("the filtered Project Browser child must exist");
            let scope = sys::igGetIDWithSeed_Str(path.as_ptr(), std::ptr::null(), (*window).ID);
            let id = sys::igGetIDWithSeed_Str(label.as_ptr(), std::ptr::null(), scope);
            sys::igFocusWindow(window, 0);
            sys::igSetFocusID(id, window);
            sys::igSetNavCursorVisible(true);
            id
        }
    });
    draw_browser(&mut context, &mut frontend, &mut application);
    context.binding().with_bound_context(|| {
        // SAFETY: the current Context owns the just-rendered navigation facts.
        unsafe {
            let current = &*sys::igGetCurrentContext();
            assert_eq!(current.NavId, image_id);
            assert!(
                current.NavIdIsAlive,
                "the real PNG Selectable must have been submitted"
            );
        }
    });
    press_browser_key(&mut context, &mut frontend, &mut application, Key::Enter);
    assert_eq!(
        current_frame(&application)
            .project_browser()
            .selected_path(),
        Some(image)
    );
    application
}

fn draw_browser(
    context: &mut Context,
    frontend: &mut ImguiEditorFrontend,
    application: &mut EditorApplication,
) {
    if let Some(batch) = imgui::draw(context, frontend, application) {
        frontend.submit_and_drive(application, batch, Duration::ZERO);
    }
}

fn press_browser_key(
    context: &mut Context,
    frontend: &mut ImguiEditorFrontend,
    application: &mut EditorApplication,
    key: Key,
) {
    context.io_mut().add_key_event(key, true);
    draw_browser(context, frontend, application);
    context.io_mut().add_key_event(key, false);
    draw_browser(context, frontend, application);
}

fn settle_image_work(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let frame = current_frame(application);
        if !frame.project_browser().has_pending_work() && !frame.image_import_work_pending() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the ordinary source index/image transaction did not settle"
        );
        application.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        thread::sleep(Duration::from_millis(5));
    }
}

fn sprite_image(frame: &EditorFrameModel) -> &ComponentValue {
    selected_component(frame, SPRITE)
        .fields
        .iter()
        .find(|field| field.id.as_str() == "material.image")
        .unwrap()
        .value
        .as_ref()
        .unwrap()
}

fn open_application(root: &Path) -> EditorApplication {
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    let _ = application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match application.state() {
            EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded => {
                return application;
            }
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance_project_open();
            }
            state => panic!("the ordinary game project did not open: {state:?}"),
        }
        thread::sleep(Duration::from_millis(1));
    }
    panic!("the ordinary game project did not publish its workspace");
}

fn current_frame(application: &EditorApplication) -> Arc<EditorFrameModel> {
    application
        .current_frame()
        .expect("the ordinary Editor must retain its workspace")
}

fn dispatch(application: &mut EditorApplication, action: EditorAction) -> Arc<EditorFrameModel> {
    let frame = current_frame(application);
    let update = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [action]),
        Duration::ZERO,
    );
    assert!(update.update().actions_observed());
    let deadline = Instant::now() + Duration::from_secs(10);
    while current_frame(application)
        .persistence()
        .operation()
        .is_some()
    {
        assert!(Instant::now() < deadline, "persistence did not settle");
        let frame = current_frame(application);
        application.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        thread::sleep(Duration::from_millis(1));
    }
    current_frame(application)
}

fn select(application: &mut EditorApplication, entity: &str) {
    let frame = current_frame(application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: scene.document,
            entity: Some(SceneEntityId::new(entity).unwrap()),
        }),
    );
}

fn selected_component<'a>(
    frame: &'a EditorFrameModel,
    id: &str,
) -> &'a SceneInspectorComponentView {
    frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .expect("the requested entity must be selected")
        .components
        .iter()
        .find(|component| component.component.as_str() == id)
        .expect("the selected entity must expose the requested schema component")
}

fn camera_height(frame: &EditorFrameModel) -> f64 {
    float_field(frame, CAMERA, "viewport_height")
}

fn collectible_radius(frame: &EditorFrameModel) -> f64 {
    float_field(frame, COLLECTIBLE, "radius")
}

fn float_field(frame: &EditorFrameModel, component: &str, field: &str) -> f64 {
    match selected_component(frame, component)
        .fields
        .iter()
        .find(|candidate| candidate.id.as_str() == field)
        .unwrap()
        .value
        .as_ref()
        .unwrap()
    {
        ComponentValue::F64(value) => value.get(),
        value => panic!("{component}.{field} must remain an f64: {value:?}"),
    }
}

fn collectible_record(path: &Path, entity_id: &str) -> Value {
    let scene: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    scene["payload"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == entity_id)
        .unwrap()["components"]
        .get(COLLECTIBLE)
        .expect("the entity retains its authored Collectible record")
        .clone()
}

fn play(application: &mut EditorApplication) {
    let frame = current_frame(application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    dispatch(
        application,
        EditorAction::Play(EditorPlayAction::Play {
            document: scene.document,
            base_revision: scene.revision,
        }),
    );
}

fn stop(application: &mut EditorApplication) {
    dispatch(application, EditorAction::Play(EditorPlayAction::Stop));
    wait_external(
        application,
        EditorExternalPlayState::Idle,
        EditorPlayOperation::Stop,
    );
}

fn wait_external(
    application: &mut EditorApplication,
    expected: EditorExternalPlayState,
    operation: EditorPlayOperation,
) -> Arc<EditorFrameModel> {
    let deadline = Instant::now()
        + Duration::from_secs(if operation == EditorPlayOperation::Stop {
            30
        } else {
            600
        });
    loop {
        let frame = current_frame(application);
        let external = frame
            .play()
            .external()
            .expect("ordinary Play uses external supervision");
        let result_matches = if expected == EditorExternalPlayState::Faulted {
            matches!(frame.play().result(), Some(EditorPlayOperationResult::Failed { operation: observed, .. }) if observed == operation)
        } else {
            matches!(frame.play().result(), Some(EditorPlayOperationResult::Applied { operation: observed, .. }) if observed == operation)
        };
        if external.state() == expected && result_matches {
            return frame;
        }
        assert!(
            !(external.state() == EditorExternalPlayState::Faulted
                && expected == EditorExternalPlayState::Running),
            "unexpected Play fault: {:?}",
            frame.play()
        );
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected:?}/{operation:?}: {:?}",
            frame.play()
        );
        application.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        thread::sleep(Duration::from_millis(5));
    }
}

fn close_application(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    if !application.request_exit().exit_ready() {
        loop {
            let frame = current_frame(application);
            if application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
                .update()
                .exit_ready()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "ordinary Editor exit did not finish physical cleanup"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    assert!(application.current_frame().is_none());
}

struct SourceRestore {
    path: PathBuf,
    original: Vec<u8>,
    restored: bool,
}

impl SourceRestore {
    fn new(path: PathBuf) -> Self {
        Self {
            original: fs::read(&path).unwrap(),
            path,
            restored: false,
        }
    }

    fn restore(&mut self) {
        fs::write(&self.path, &self.original)
            .expect("restore the disposable project's desktop source");
        self.restored = true;
    }
}

impl Drop for SourceRestore {
    fn drop(&mut self) {
        if !self.restored {
            if let Err(error) = fs::write(&self.path, &self.original) {
                eprintln!("could not restore the disposable desktop source: {error}");
            }
        }
    }
}
