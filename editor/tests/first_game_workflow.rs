#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable as _};
use nara::{
    asset::AssetRef,
    reflect::{ComponentFieldId, ComponentTypeId, ComponentValue},
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentMutation, EditorExternalPlayCompilerLevel,
        EditorExternalPlayMessageKind, EditorExternalPlayState, EditorFrameModel,
        EditorImageColorSpace, EditorImageImportAttemptView, EditorImageImportAvailabilityView,
        EditorImageImportFreshness, EditorImageImportMetadataView, EditorImageImportSettings,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceResult, EditorPlayAction,
        EditorPlayOperation, EditorPlayOperationResult, EditorRuntimeAvailability,
        SceneInspectorCommand, SceneInspectorComponentView, SceneInspectorReadOnlyReason,
    },
};
use nara_editor::{EditorApplication, EditorApplicationState, EditorLaunchOptions};
use serde_json::Value;

const CAMERA: &str = "nara.render.Camera2d";
const COLLECTIBLE: &str = "collect-game.Collectible";
const SPRITE: &str = "nara.sprite.Sprite";
const IMAGE_SOURCE: &str = "assets/textures/token.png";
const COMPILER_ERROR: &str = "first-game workflow injected compiler error";

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
    let original_collectible = collectible_record(&scene_path);
    let mut application = open_application(&root);
    assert_eq!(
        current_frame(&application).runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    );
    let unavailable = application
        .active_authoring_unavailable_record_manifest()
        .unwrap();
    assert!(unavailable.total_count() > 0);
    select(&mut application, "gem-left");
    assert_eq!(
        selected_component(&current_frame(&application), COLLECTIBLE)
            .access
            .read_only_reason(),
        Some(SceneInspectorReadOnlyReason::KnownUnbound),
    );
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
    assert_eq!(collectible_record(&scene_path), original_collectible);
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(unavailable.clone())
    );
    let acknowledged = dispatch(
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
    );
    assert!(acknowledged.persistence().result().is_none());
    close_application(&mut application);

    let mut application = open_application(&root);
    select(&mut application, "gem-left");
    assert_eq!(sprite_image(&current_frame(&application)), &assigned_image);
    select(&mut application, "main-camera");
    assert_eq!(camera_height(&current_frame(&application)), edited_height);
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(unavailable)
    );
    assert_eq!(collectible_record(&scene_path), original_collectible);
    eprintln!("camera edit/undo/redo/save/reopen: {:?}", editing.elapsed());

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
    select(&mut application, "gem-left");
    assert_eq!(
        selected_component(&current_frame(&application), COLLECTIBLE)
            .access
            .read_only_reason(),
        Some(SceneInspectorReadOnlyReason::KnownUnbound)
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
    assert_eq!(collectible_record(&scene_path), original_collectible);
}

fn reimport_and_assign_image(mut application: EditorApplication, root: &Path) -> EditorApplication {
    let pixels = fs::read(root.join(IMAGE_SOURCE)).unwrap();
    let mut metadata = SourceRestore::new(root.join(format!("{IMAGE_SOURCE}.meta")));
    settle_image_work(&mut application);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, application: &mut EditorApplication| {
                if let Some(batch) = application.show(ui) {
                    application.submit_and_drive(batch, Duration::ZERO);
                }
            },
            application,
        );
    harness.get_by_label("Project").click();
    harness.run();
    harness.get_by_role(Role::TextInput).focus();
    harness.run();
    harness.get_by_role(Role::TextInput).type_text("token.png");
    harness.run();
    assert_eq!(
        current_frame(harness.state())
            .project_browser()
            .path_filter(),
        "token.png",
        "the ordinary Project filter must receive text before selecting its PNG row"
    );
    harness
        .get_by_label("AST  assets/textures/token.png")
        .click();
    harness.run();
    let mut application = harness.into_state();
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
    for _ in 0..16 {
        match application.state() {
            EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded => {
                return application;
            }
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance_project_open();
            }
            state => panic!("the ordinary game project did not open: {state:?}"),
        }
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
    assert!(update.actions_observed());
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
    match selected_component(frame, CAMERA)
        .fields
        .iter()
        .find(|field| field.id.as_str() == "viewport_height")
        .unwrap()
        .value
        .as_ref()
        .unwrap()
    {
        ComponentValue::F64(value) => value.get(),
        value => panic!("camera viewport_height must remain an f64: {value:?}"),
    }
}

fn collectible_record(path: &Path) -> Value {
    let scene: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    scene["payload"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == "gem-left")
        .unwrap()["components"]
        .get(COLLECTIBLE)
        .expect("gem-left retains its authored Collectible record")
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
