#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[cfg(windows)]
use std::fs::OpenOptions;

use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable as _};
use nara::{
    ProductRecipe,
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions, RelativePath,
        TrustMode,
    },
    project_host::{
        EditorGameStartRequest, EditorGameViewCandidate, EditorGameViewCompletion,
        TrustedCompiledEditorProjectIntent,
    },
    project_workflow::{
        LocalDependencyParent, NaraDependencySource, ProjectCheckRequest, ProjectCreationOutcome,
        ProjectCreationRequest, ProjectSlug, StartupContentMemberKind, check_project,
        prepare_desktop_2d_project,
    },
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    render::Extent2d,
    scene::{
        SceneComponentRecord, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentMutation, EditorFieldFeedbackReason,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceResult, EditorPlayAction,
        EditorPlayOperation, EditorPlayOperationResult, EditorPlayState, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentResult, SceneInspectorCommand,
    },
};
use nara_editor::{EditorApplication, EditorLaunchOptions};
use serde_json::Value;

const PROJECT_SLUG: &str = "editor-authoring-loop";
const CREATED_ENTITY: &str = "editor_entity_1";
const GAME_VIEW_PROBE_ENTITY: &str = "game-view-probe";
const SPRITE_COMPONENT: &str = "nara.sprite.Sprite";
const TRANSFORM_COMPONENT: &str = "nara.transform.Transform2d";
const TRANSLATION_X_FIELD: &str = "translation.x";
const PAUSED_TRANSLATION_X: f64 = 48.0;
const EDITED_TRANSLATION_X: f64 = 128.0;
const STALE_TRANSLATION_X: f64 = 256.0;
const EXPORT_DESTINATION: &str = "editor-authoring-candidate";
const PRODUCT_OBSERVATION_ATTEMPT_ENV: &str = "NARA_STARTER_OBSERVATION_ATTEMPT";
const PRODUCT_OBSERVATION_FILE_ENV: &str = "NARA_STARTER_OBSERVATION_FILE";
const EXPORT_OBSERVATION_ATTEMPT_ENV: &str = "NARA_EXPORT_OBSERVATION_ATTEMPT";
const EXPORT_OBSERVATION_FILE_ENV: &str = "NARA_EXPORT_OBSERVATION_FILE";
static NEXT_TRAJECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
fn starter_game_view_preserves_unsaved_source_and_explicit_play_lifecycle() {
    let root = TrajectoryRoot::new();
    let project = create_project(&root);
    let scene_path = project.join("scenes/startup.scene.json");
    let source_before = fs::read(&scene_path).unwrap();
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project),
        TrustedCompiledEditorProjectIntent::new().with_profile("desktop"),
    )
    .expect("the desktop starter must open with its explicit desktop profile");
    application.set_scene_preview_extent(Extent2d::new(960, 540));

    let opened = current_frame(&application);
    let opened_scene = opened.workspace().active_scene.as_ref().unwrap();
    let document = opened_scene.document;
    let edited = dispatch(
        &mut application,
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyScenePatch {
                document,
                base_revision: opened_scene.revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(
                        SceneEntityId::new(GAME_VIEW_PROBE_ENTITY).unwrap(),
                    )
                    .with_component(
                        ComponentTypeId::new(TRANSFORM_COMPONENT),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            transform_value(0.0, 0.0),
                        ),
                    )
                    .with_component(
                        ComponentTypeId::new(SPRITE_COMPONENT),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            color_sprite_value(),
                        ),
                    ),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            },
        )],
    );
    let edited_scene = edited.workspace().active_scene.as_ref().unwrap();
    let edited_revision = edited_scene.revision;
    assert!(edited_scene.dirty);
    assert!(
        edited_scene
            .editor
            .inspector
            .entities
            .iter()
            .any(|entity| entity.id.as_str() == GAME_VIEW_PROBE_ENTITY)
    );
    assert_eq!(fs::read(&scene_path).unwrap(), source_before);

    let preview = take_scene_preview_candidate(&mut application, edited_revision);
    assert_eq!(preview.document(), document);
    assert_eq!(preview.revision(), edited_revision);
    let preview_hit = preview
        .hits()
        .iter()
        .find(|hit| hit.entity().as_str() == GAME_VIEW_PROBE_ENTITY)
        .expect("the unsaved color sprite must be visible in Scene Preview");
    assert!(preview_hit.contains_clip_point([0.0, 0.0]));
    drop(preview);
    select_entity(&mut application, document, GAME_VIEW_PROBE_ENTITY);

    dispatch(
        &mut application,
        [EditorAction::Play(EditorPlayAction::Play {
            document,
            base_revision: edited_revision,
        })],
    );
    let first_start = activate_pending_game(&mut application);
    assert_eq!(first_start.source_revision(), edited_revision);
    assert!(first_start.captures_sprites());
    assert!(!first_start.captures_runtime_ui());

    let first_candidate = take_game_candidate(&mut application);
    assert_eq!(first_candidate.identity().start(), first_start);
    let first_frame = first_candidate.identity().frame_index();
    let first_identity = first_candidate.identity();
    let mut first_parts = first_candidate.into_parts();
    let first_batches = first_parts
        .take_sprite_batches()
        .expect("the desktop starter captures its unsaved color sprite");
    assert_eq!(first_batches.total_instances(), 1);
    let first_center = first_batches.as_slice()[0].instances[0].center;
    assert_eq!(first_center.x, 0.0);
    drop(first_parts);
    complete_plain_identity(&mut application, first_identity);
    drive_empty(&mut application);
    assert_eq!(
        current_frame(&application).play().state(),
        EditorPlayState::Running
    );

    dispatch(
        &mut application,
        [EditorAction::Play(EditorPlayAction::Pause)],
    );
    assert_eq!(
        current_frame(&application).play().state(),
        EditorPlayState::Paused
    );
    let paused_candidate = take_game_candidate(&mut application);
    assert!(paused_candidate.identity().frame_index() > first_frame);
    complete_plain_candidate(&mut application, paused_candidate);

    let resized_extent = Extent2d::new(800, 450).unwrap();
    assert!(application.resize_game_view(resized_extent).unwrap());
    let resized_start = application.active_game_start_request().unwrap();
    assert_eq!(resized_start.geometry_generation(), 2);
    drive_empty(&mut application);
    let resized_candidate = take_game_candidate(&mut application);
    assert_eq!(resized_candidate.identity().start(), resized_start);
    assert_eq!(
        resized_candidate.packet().views()[0].viewport.extent(),
        resized_extent
    );
    complete_plain_candidate(&mut application, resized_candidate);

    dispatch(
        &mut application,
        [EditorAction::Play(EditorPlayAction::StepFixedTick)],
    );
    assert_eq!(
        current_frame(&application).play().state(),
        EditorPlayState::Stepping
    );
    let step_candidate = take_game_candidate(&mut application);
    let step_identity = step_candidate.identity();
    complete_plain_candidate(&mut application, step_candidate);
    drive_empty(&mut application);
    assert_eq!(
        current_frame(&application).play().state(),
        EditorPlayState::Paused
    );
    assert!(matches!(
        current_frame(&application).play().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::StepFixedTick,
            generation: Some(generation),
        }) if generation == step_identity.runtime_generation().get()
    ));

    application
        .set_game_view_source_active(false, "the starter trajectory selected Scene View")
        .unwrap();
    let paused = current_frame(&application);
    let transform = selected_f64_field(&paused, TRANSFORM_COMPONENT, TRANSLATION_X_FIELD);
    let paused_edit = dispatch(
        &mut application,
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision: paused.workspace().active_scene.as_ref().unwrap().revision,
                command: SceneInspectorCommand::SetField {
                    entity: SceneEntityId::new(GAME_VIEW_PROBE_ENTITY).unwrap(),
                    component: ComponentTypeId::new(TRANSFORM_COMPONENT),
                    component_version: transform.component_version,
                    field: ComponentFieldId::new(TRANSLATION_X_FIELD),
                    value: ComponentValue::f64(PAUSED_TRANSLATION_X).unwrap(),
                },
            },
        )],
    );
    let paused_edit_revision = paused_edit
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .revision;
    let old_generation = paused_edit.play().generation().unwrap();
    assert_eq!(paused_edit.play().state(), EditorPlayState::Paused);
    assert!(paused_edit.play().is_out_of_date());
    assert_eq!(
        selected_f64_field(&paused_edit, TRANSFORM_COMPONENT, TRANSLATION_X_FIELD).value,
        PAUSED_TRANSLATION_X
    );

    let harness = editor_harness(application);
    let _ = harness.get_by_label_contains("Game View is out of date");
    application = harness.into_state();

    dispatch(
        &mut application,
        [EditorAction::Play(EditorPlayAction::Restart)],
    );
    let restarted = activate_pending_game(&mut application);
    assert_eq!(restarted.source_revision(), paused_edit_revision);
    assert_ne!(restarted.runtime_generation().get(), old_generation);
    let restarted_candidate = take_game_candidate(&mut application);
    assert_eq!(restarted_candidate.identity().start(), restarted);
    let restarted_identity = restarted_candidate.identity();
    let mut restarted_parts = restarted_candidate.into_parts();
    let restarted_batches = restarted_parts
        .take_sprite_batches()
        .expect("Restart must capture the edited color sprite");
    assert_eq!(restarted_batches.total_instances(), 1);
    assert!(restarted_batches.as_slice()[0].instances[0].center.x > first_center.x);
    drop(restarted_parts);
    complete_plain_identity(&mut application, restarted_identity);
    drive_empty(&mut application);
    assert_eq!(
        current_frame(&application).play().state(),
        EditorPlayState::Running
    );
    assert!(!current_frame(&application).play().is_out_of_date());

    dispatch(
        &mut application,
        [EditorAction::Play(EditorPlayAction::Stop)],
    );
    drive_until_play_state(&mut application, EditorPlayState::Empty);
    let stopped = current_frame(&application);
    assert!(stopped.workspace().active_scene.as_ref().unwrap().dirty);
    assert_eq!(fs::read(&scene_path).unwrap(), source_before);

    let saved = dispatch(
        &mut application,
        [EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: stopped.workspace().active_scene.as_ref().unwrap().revision,
        })],
    );
    assert!(matches!(
        saved.persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document: saved_document,
            revision,
            ..
        }) if saved_document == document && revision == paused_edit_revision
    ));
    assert!(!saved.workspace().active_scene.as_ref().unwrap().dirty);
    assert_ne!(fs::read(&scene_path).unwrap(), source_before);

    dispatch(
        &mut application,
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )],
    );
    let closed = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument { document }),
        )],
    );
    assert!(closed.workspace().active_document.is_none());
    let closed = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )],
    );
    assert_eq!(
        closed.reopen_target(),
        nara::tooling::EditorReopenTarget::Unavailable
    );
    drop(application);

    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project),
        TrustedCompiledEditorProjectIntent::new().with_profile("desktop"),
    )
    .expect("a new Editor session must reopen the saved starter Scene");
    application.set_scene_preview_extent(Extent2d::new(960, 540));
    let reopened_document = current_frame(&application)
        .workspace()
        .active_document
        .unwrap();
    select_entity(&mut application, reopened_document, GAME_VIEW_PROBE_ENTITY);
    let reopened = current_frame(&application);
    assert!(!reopened.workspace().active_scene.as_ref().unwrap().dirty);
    assert_eq!(
        selected_f64_field(&reopened, TRANSFORM_COMPONENT, TRANSLATION_X_FIELD).value,
        PAUSED_TRANSLATION_X
    );
}

#[test]
#[ignore = "runs one nested Cargo build, generated product, and real Winit/WGPU export acceptance"]
fn starter_edit_crosses_editor_check_generated_product_and_export() {
    let root = TrajectoryRoot::new();
    let project = create_project(&root);
    let scene_path = project.join("scenes/startup.scene.json");

    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the published starter must open in the private Editor application");
    application.set_scene_preview_extent(Extent2d::new(960, 540));
    let opened = current_frame(&application);
    let opened_scene = opened
        .workspace()
        .active_scene
        .as_ref()
        .expect("the starter opens its configured startup Scene");
    let document = opened_scene.document;
    let initial_revision = opened_scene.revision;
    assert!(!opened_scene.dirty);

    let mut harness = editor_harness(application);
    harness.get_by_label("+ Root").click();
    harness.run();
    let composed = current_frame(harness.state());
    let composed_scene = composed.workspace().active_scene.as_ref().unwrap();
    let composed_revision = composed_scene.revision;
    let created_entity = SceneEntityId::new(CREATED_ENTITY).unwrap();
    assert_ne!(composed_revision, initial_revision);
    assert_eq!(
        composed_scene.editor.inspector.selected_entity.as_ref(),
        Some(&created_entity)
    );
    assert!(composed_scene.dirty);
    assert_eq!(transform_x_field(&composed).value, 0.0);

    harness
        .get_by_role_and_label(Role::TextInput, TRANSLATION_X_FIELD)
        .focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run();
    harness
        .get_by_role_and_label(Role::TextInput, TRANSLATION_X_FIELD)
        .type_text("128");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    let edited = current_frame(harness.state());
    let edited_scene = edited.workspace().active_scene.as_ref().unwrap();
    let edited_revision = edited_scene.revision;
    let field = transform_x_field(&edited);
    assert_ne!(edited_revision, composed_revision);
    assert!(edited_scene.dirty);
    assert_eq!(field.value, EDITED_TRANSLATION_X);

    let candidate = harness
        .state_mut()
        .take_scene_preview_candidate()
        .expect("the accepted Transform edit publishes one preview candidate");
    assert_eq!(candidate.document(), document);
    assert_eq!(candidate.revision(), edited_revision);
    let hit = candidate
        .hits()
        .iter()
        .find(|hit| hit.entity() == &created_entity)
        .expect("the composed visible entity contributes one preview hit");
    assert!(hit.contains_clip_point([0.2, 0.0]));

    let mut application = harness.into_state();

    let stale = dispatch(
        &mut application,
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision: composed_revision,
                command: SceneInspectorCommand::SetField {
                    entity: created_entity.clone(),
                    component: ComponentTypeId::new(TRANSFORM_COMPONENT),
                    component_version: field.component_version,
                    field: ComponentFieldId::new(TRANSLATION_X_FIELD),
                    value: ComponentValue::f64(STALE_TRANSLATION_X).unwrap(),
                },
            },
        )],
    );
    assert_eq!(
        stale.workspace().active_scene.as_ref().unwrap().revision,
        edited_revision
    );
    assert_eq!(transform_x_field(&stale).value, EDITED_TRANSLATION_X);
    assert_eq!(
        stale.field_feedback().unwrap().reason(),
        EditorFieldFeedbackReason::StaleAction
    );
    assert!(has_diagnostic(&stale, "editor.action.stale-document"));

    let mut harness = editor_harness(application);
    harness.get_by_label("Undo").click();
    harness.run();
    let undone = current_frame(harness.state());
    let undone_scene = undone.workspace().active_scene.as_ref().unwrap();
    assert!(undone_scene.dirty);
    assert_eq!(transform_x_field(&undone).value, 0.0);

    harness.get_by_label("Redo").click();
    harness.run();
    let redone = current_frame(harness.state());
    let redone_scene = redone.workspace().active_scene.as_ref().unwrap();
    assert!(redone_scene.dirty);
    assert_eq!(transform_x_field(&redone).value, EDITED_TRANSLATION_X);

    harness.get_by_label("Save").click();
    harness.run();
    let saved = current_frame(harness.state());
    assert!(matches!(
        saved.persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document: saved_document,
            revision,
            ..
        }) if saved_document == document && revision == redone_scene.revision
    ));
    assert!(!saved.workspace().active_scene.as_ref().unwrap().dirty);
    let saved_scene_digest = ContentDigest::of_bytes(&fs::read(&scene_path).unwrap());
    let mut application = harness.into_state();

    let acknowledged_save = dispatch(
        &mut application,
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )],
    );
    assert!(acknowledged_save.persistence().result().is_none());

    let closed = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument { document }),
        )],
    );
    assert!(closed.workspace().scenes.is_empty());
    assert_eq!(
        closed.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document },
        })
    );

    let ready_to_reopen = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )],
    );
    assert_eq!(
        ready_to_reopen.reopen_target(),
        nara::tooling::EditorReopenTarget::Unavailable
    );
    drop(application);

    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("a new Editor session must reopen the saved starter Scene");
    application.set_scene_preview_extent(Extent2d::new(960, 540));
    let reopened_document = current_frame(&application)
        .workspace()
        .active_document
        .unwrap();
    let mut harness = editor_harness(application);
    harness.get_by_label_contains(CREATED_ENTITY).click();
    harness.run();
    let reopened = current_frame(harness.state());
    assert!(!reopened.workspace().active_scene.as_ref().unwrap().dirty);
    assert_eq!(transform_x_field(&reopened).value, EDITED_TRANSLATION_X);
    let reopened_scene = reopened.workspace().active_scene.as_ref().unwrap();
    let created_row = reopened_scene
        .editor
        .inspector
        .entities
        .iter()
        .find(|row| row.id == created_entity)
        .expect("reopen reconstructs the composed entity in the canonical tree");
    assert!(created_row.parent.is_none());

    let mut application = harness.into_state();

    dispatch(
        &mut application,
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )],
    );
    let finally_closed = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                document: reopened_document,
            }),
        )],
    );
    assert!(finally_closed.workspace().scenes.is_empty());
    drop(application);

    let checked = check_project(ProjectCheckRequest::new(
        readable_directory(&project),
        ProductRecipe::new(),
    ))
    .expect("the ordinary project check must admit the saved Editor source");
    let scene_member = checked
        .members()
        .iter()
        .find(|member| {
            member.kind() == StartupContentMemberKind::Scene
                && member.path().as_str() == "scenes/startup.scene.json"
        })
        .expect("the checked startup closure contains the edited Scene");
    assert_eq!(scene_member.digest(), saved_scene_digest);
    let manifest_digest = checked.manifest_digest();
    let content_digest = checked.content_digest();
    drop(checked);

    let product_observation = run_generated_headless(&root, &project);
    assert_product_observation(
        &product_observation,
        "headless",
        "default",
        manifest_digest,
        content_digest,
    );

    let export_observation = run_export(&root, &project);
    assert_digest_value(
        export_observation
            .get("startup_content")
            .expect("export observation contains startup content"),
        content_digest,
    );
    let products = export_observation
        .get("products")
        .and_then(Value::as_array)
        .expect("export observation contains accepted products");
    assert_eq!(products.len(), 2);
    for (product, (target, profile)) in products
        .iter()
        .zip([("headless", "default"), ("desktop", "desktop")])
    {
        assert_eq!(product.get("target").and_then(Value::as_str), Some(target));
        assert_eq!(
            product.get("profile").and_then(Value::as_str),
            Some(profile)
        );
        assert_digest_value(product.get("manifest").unwrap(), manifest_digest);
        assert_digest_value(product.get("content").unwrap(), content_digest);
    }
    assert_eq!(
        ContentDigest::of_bytes(
            &fs::read(
                root.export_parent()
                    .join(EXPORT_DESTINATION)
                    .join("project/scenes/startup.scene.json"),
            )
            .unwrap(),
        ),
        saved_scene_digest
    );
}

#[derive(Clone)]
struct TransformXField {
    component_version: nara::reflect::ComponentSchemaVersion,
    value: f64,
}

fn create_project(root: &TrajectoryRoot) -> PathBuf {
    let checkout = writable_directory(root.checkout());
    let publication_parent = checkout
        .open_directory(root.project_parent_relation())
        .unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        publication_parent,
        ProjectSlug::new(PROJECT_SLUG).unwrap(),
        NaraDependencySource::local(
            checkout,
            LocalDependencyParent::Descendant(root.project_parent_relation().clone()),
        ),
    ))
    .unwrap();
    let ProjectCreationOutcome::Created(receipt) = prepared.publish() else {
        panic!("the ordinary starter publication must create an admitted project");
    };
    let project = root.project_parent().join(PROJECT_SLUG);
    assert_eq!(receipt.plan().slug().as_str(), PROJECT_SLUG);
    project
}

fn editor_harness(application: EditorApplication) -> Harness<'static, EditorApplication> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, application| {
                if let Some(batch) = application.show(ui) {
                    application.submit_and_drive(batch, Duration::ZERO);
                }
            },
            application,
        )
}

fn current_frame(
    application: &EditorApplication,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    application
        .current_frame()
        .expect("the trusted compiled Editor retains one published workspace frame")
}

fn dispatch(
    application: &mut EditorApplication,
    actions: impl IntoIterator<Item = EditorAction>,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let frame = current_frame(application);
    application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), actions),
        Duration::ZERO,
    );
    current_frame(application)
}

fn drive_empty(application: &mut EditorApplication) {
    let _ = dispatch(application, std::iter::empty());
}

fn drive_until_play_state(application: &mut EditorApplication, expected: EditorPlayState) {
    for _ in 0..32 {
        if current_frame(application).play().state() == expected {
            return;
        }
        drive_empty(application);
    }
    panic!(
        "the Editor did not reach {expected:?}; current frame: {:?}",
        current_frame(application).play()
    );
}

fn activate_pending_game(application: &mut EditorApplication) -> EditorGameStartRequest {
    let request = (0..32)
        .find_map(|_| {
            let request = application.pending_game_start_request();
            if request.is_none() {
                drive_empty(application);
            }
            request
        })
        .expect("the Play request must prepare one exact inactive Game target reservation");
    let activation = application
        .prepare_game_start_activation(request)
        .expect("the exact inactive Game target reservation must match the prepared Runtime");
    application.commit_game_start_activation(activation);
    drive_empty(application);
    request
}

fn take_game_candidate(application: &mut EditorApplication) -> EditorGameViewCandidate {
    for _ in 0..32 {
        if let Some(candidate) = application
            .take_game_view_candidate()
            .expect("the active starter Runtime must retain its Game candidate authority")
        {
            return candidate;
        }
        drive_empty(application);
    }
    panic!("the active starter Runtime did not publish a Game candidate");
}

fn complete_plain_candidate(
    application: &mut EditorApplication,
    candidate: EditorGameViewCandidate,
) {
    let identity = candidate.identity();
    drop(candidate);
    complete_plain_identity(application, identity);
}

fn complete_plain_identity(
    application: &mut EditorApplication,
    identity: nara::project_host::EditorGameViewCandidateIdentity,
) {
    application
        .complete_game_view_candidate(EditorGameViewCompletion::without_ui(identity))
        .expect("the exact starter Game candidate completion must return once");
}

fn take_scene_preview_candidate(
    application: &mut EditorApplication,
    expected_revision: nara::scene::SceneAuthoringRevision,
) -> nara::project_host::EditorPreviewCandidate {
    for _ in 0..16 {
        if let Some(candidate) = application.take_scene_preview_candidate()
            && candidate.revision() == expected_revision
        {
            return candidate;
        }
        drive_empty(application);
    }
    panic!("the accepted starter edit did not publish a Scene preview candidate");
}

fn select_entity(
    application: &mut EditorApplication,
    document: nara::tooling::EditorDocumentId,
    id: &str,
) {
    let _ = dispatch(
        application,
        [EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document,
                entity: Some(SceneEntityId::new(id).unwrap()),
            },
        )],
    );
}

fn selected_f64_field(
    frame: &nara::tooling::EditorFrameModel,
    component_id: &str,
    field_id: &str,
) -> TransformXField {
    let selected = frame
        .workspace()
        .active_scene
        .as_ref()
        .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
        .expect("the requested starter entity is selected in the Inspector");
    let component = selected
        .components
        .iter()
        .find(|component| component.component.as_str() == component_id)
        .expect("the selected starter entity exposes the requested component");
    let field = component
        .fields
        .iter()
        .find(|field| field.id.as_str() == field_id)
        .expect("the selected starter component exposes the requested field");
    let ComponentValue::F64(value) = field
        .value
        .as_ref()
        .expect("the requested starter field has a concrete authored value")
    else {
        panic!("the requested starter field must remain an f64");
    };
    TransformXField {
        component_version: component.schema_version,
        value: value.get(),
    }
}

fn transform_value(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        (
            "translation",
            ComponentValue::map([
                ("x", ComponentValue::f64(x).unwrap()),
                ("y", ComponentValue::f64(y).unwrap()),
            ]),
        ),
        ("rotation", ComponentValue::f64(0.0).unwrap()),
        (
            "scale",
            ComponentValue::map([
                ("x", ComponentValue::f64(1.0).unwrap()),
                ("y", ComponentValue::f64(1.0).unwrap()),
            ]),
        ),
    ])
}

fn color_sprite_value() -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(96.0).unwrap()),
                ("y", ComponentValue::f64(96.0).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([(
                "tint",
                ComponentValue::map([
                    ("r", ComponentValue::f64(0.2).unwrap()),
                    ("g", ComponentValue::f64(0.7).unwrap()),
                    ("b", ComponentValue::f64(1.0).unwrap()),
                    ("a", ComponentValue::f64(1.0).unwrap()),
                ]),
            )]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn transform_x_field(frame: &nara::tooling::EditorFrameModel) -> TransformXField {
    selected_f64_field(frame, TRANSFORM_COMPONENT, TRANSLATION_X_FIELD)
}

fn has_diagnostic(frame: &nara::tooling::EditorFrameModel, code: &str) -> bool {
    frame
        .diagnostics()
        .iter()
        .any(|entry| entry.code().as_str() == code)
}

fn run_generated_headless(root: &TrajectoryRoot, project: &Path) -> Value {
    let attempt = format!("editor-source-{}", std::process::id());
    let observation = root.path().join("source-headless.json");
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(root.unrelated_cwd())
        .args([
            "run",
            "--manifest-path",
            project.join("Cargo.toml").to_str().unwrap(),
            "--bin",
            "headless",
        ])
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.product_target())
        .env(PRODUCT_OBSERVATION_ATTEMPT_ENV, &attempt)
        .env(PRODUCT_OBSERVATION_FILE_ENV, &observation);
    expect_success(command, "generated headless product");
    let value = read_json(&observation);
    assert_eq!(
        value.get("attempt").and_then(Value::as_str),
        Some(attempt.as_str())
    );
    value
}

fn run_export(root: &TrajectoryRoot, project: &Path) -> Value {
    let cli = build_nara_cli(root.checkout());
    let attempt = format!("editor-export-{}", std::process::id());
    let observation = root.path().join("export.json");
    let mut command = Command::new(cli);
    command
        .current_dir(project)
        .args([
            "export",
            ".",
            "--parent",
            root.export_parent().to_str().unwrap(),
            "--destination",
            EXPORT_DESTINATION,
        ])
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.product_target())
        .env(EXPORT_OBSERVATION_ATTEMPT_ENV, &attempt)
        .env(EXPORT_OBSERVATION_FILE_ENV, &observation);
    expect_success(command, "source-carrying export");
    let value = read_json(&observation);
    assert_eq!(
        value.get("attempt").and_then(Value::as_str),
        Some(attempt.as_str())
    );
    value
}

fn build_nara_cli(checkout: &Path) -> PathBuf {
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(checkout)
        .args([
            "build",
            "--locked",
            "--no-default-features",
            "--features",
            "project-workflow-cli",
            "--bin",
            "nara",
        ])
        .env("CARGO_BUILD_JOBS", "1");
    expect_success(command, "public nara CLI build");
    checkout
        .join("target/debug")
        .join(format!("nara{}", std::env::consts::EXE_SUFFIX))
}

fn assert_product_observation(
    value: &Value,
    target: &str,
    profile: &str,
    manifest_digest: ContentDigest,
    content_digest: ContentDigest,
) {
    let manifest_hash = digest_hex(manifest_digest);
    let content_hash = digest_hex(content_digest);
    assert_eq!(value.get("version").and_then(Value::as_u64), Some(1));
    assert_eq!(value.get("target").and_then(Value::as_str), Some(target));
    assert_eq!(value.get("profile").and_then(Value::as_str), Some(profile));
    assert_eq!(
        value.get("manifest_length").and_then(Value::as_u64),
        Some(manifest_digest.length())
    );
    assert_eq!(
        value.get("manifest_blake3").and_then(Value::as_str),
        Some(manifest_hash.as_str())
    );
    assert_eq!(
        value.get("content_length").and_then(Value::as_u64),
        Some(content_digest.length())
    );
    assert_eq!(
        value.get("content_blake3").and_then(Value::as_str),
        Some(content_hash.as_str())
    );
}

fn assert_digest_value(value: &Value, digest: ContentDigest) {
    let hash = digest_hex(digest);
    assert_eq!(
        value.get("length").and_then(Value::as_u64),
        Some(digest.length())
    );
    assert_eq!(
        value.get("blake3").and_then(Value::as_str),
        Some(hash.as_str())
    );
}

fn digest_hex(digest: ContentDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn expect_success(mut command: Command, label: &str) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{label} failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

struct TrajectoryRoot {
    checkout: PathBuf,
    target: PathBuf,
    path: PathBuf,
    project_parent: PathBuf,
    project_parent_relation: RelativePath,
    export_parent: PathBuf,
    unrelated_cwd: PathBuf,
    product_target: PathBuf,
}

impl TrajectoryRoot {
    fn new() -> Self {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        let target = checkout.join("target").canonicalize().unwrap();
        let sequence = NEXT_TRAJECTORY.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "nara-editor-authoring-loop-{}-{sequence}",
            std::process::id()
        );
        let path = target.join(&name);
        let project_parent = path.join("projects");
        let export_parent = path.join("exports");
        let unrelated_cwd = path.join("unrelated-cwd");
        let product_target = target.join("nara-editor-product-cache");
        fs::create_dir(&path).unwrap();
        fs::create_dir(&project_parent).unwrap();
        fs::create_dir(&export_parent).unwrap();
        fs::create_dir(&unrelated_cwd).unwrap();
        fs::create_dir_all(&product_target).unwrap();
        let project_parent_relation = RelativePath::new(format!("target/{name}/projects")).unwrap();
        Self {
            checkout,
            target,
            path,
            project_parent,
            project_parent_relation,
            export_parent,
            unrelated_cwd,
            product_target,
        }
    }

    fn checkout(&self) -> &Path {
        &self.checkout
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn project_parent(&self) -> &Path {
        &self.project_parent
    }

    const fn project_parent_relation(&self) -> &RelativePath {
        &self.project_parent_relation
    }

    fn export_parent(&self) -> &Path {
        &self.export_parent
    }

    fn unrelated_cwd(&self) -> &Path {
        &self.unrelated_cwd
    }

    fn product_target(&self) -> &Path {
        &self.product_target
    }
}

impl Drop for TrajectoryRoot {
    fn drop(&mut self) {
        if self.path.starts_with(&self.target) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn readable_directory(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(path, false),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap()
}

fn writable_directory(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(path, true),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

#[cfg(windows)]
fn host_directory(path: &Path, writable: bool) -> File {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(writable)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap()
}

#[cfg(unix)]
fn host_directory(path: &Path, _writable: bool) -> File {
    File::open(path).unwrap()
}
