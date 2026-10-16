#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{path::Path, time::Duration};

use nara::{
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{AuthoringFieldEdit, SceneEntityId},
};
use nara_editor::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, EguiEditorFrontend,
    workspace::{EditorPanelModel, EditorPanelTarget},
};
use nara_editor_panel_module_consumer::{
    PANEL_KIND, clear_selection, install, install_egui, selected_count,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
    EditorNavigationAction, EditorWorkspaceIntentAction,
};

#[test]
fn independent_contribution_observes_selection_dispatches_and_reopens_without_document_loss() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generic-editor/known-unbound");
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
    application.native_shell_ready();
    for _ in 0..8 {
        if application.current_frame().is_some() {
            break;
        }
        application.advance_project_open();
    }
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    install(application.panel_workspace_mut()).unwrap();
    let mut frontend = EguiEditorFrontend::new();
    install_egui(&mut frontend);
    let instance_id = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == PANEL_KIND)
        .unwrap()
        .id();
    let frame = application.current_frame().unwrap();
    let document = frame.workspace().active_document.unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(SceneEntityId::new("signal-node").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    let selected = application.current_frame().unwrap();
    let instance = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap();
    let count = selected_count(EditorPanelModel::new(&selected, instance));
    let action = clear_selection(EditorPanelModel::new(&selected, instance)).unwrap();
    let scene = selected.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            selected.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyAuthoringFieldEdit {
                    document,
                    base_revision: scene.revision,
                    edit: AuthoringFieldEdit::single(
                        SceneEntityId::new("signal-node").unwrap(),
                        ComponentTypeId::new("nara.scene.Name"),
                        ComponentSchemaVersion::ONE,
                        ComponentFieldId::new("value"),
                        ComponentValue::String("Panel lifetime proof".to_owned()),
                    )
                    .unwrap(),
                },
            )],
        ),
        Duration::ZERO,
    );
    let before_close = application.current_frame().unwrap();
    let facts = before_close.workspace().document(document).unwrap().clone();
    let source = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap()
        .target()
        .clone();
    application
        .panel_workspace_mut()
        .layout_mut()
        .close(instance_id);
    application
        .panel_workspace_mut()
        .layout_mut()
        .reopen(instance_id);
    let duplicate = application
        .panel_workspace_mut()
        .open(PANEL_KIND, EditorPanelTarget::ActiveDocument)
        .unwrap();
    let after = application.current_frame().unwrap();
    let same_document = after.workspace().document(document).unwrap() == &facts;
    let same_target = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap()
        .target()
        == &source;
    let current = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(current.revision(), [action]),
        Duration::ZERO,
    );
    let cleared = application.current_frame().unwrap();
    let is_cleared = cleared
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .is_empty();
    let context = egui::Context::default();
    let mut emitted = None;
    let output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            emitted = frontend.show(ui, &mut application);
        },
    );
    output.drop_without_applying_deltas();
    application.request_exit();
    let closing = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            closing.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Discard),
            )],
        ),
        Duration::ZERO,
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let Some(frame) = application.current_frame() else {
            break;
        };
        if application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
            .exit_ready()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(count, 1);
    assert!(facts.dirty);
    assert_eq!(facts.history.undo_depth, 1);
    assert!(same_document && same_target);
    assert_ne!(instance_id, duplicate);
    assert!(is_cleared);
    assert!(emitted.is_some());
    assert!(application.current_frame().is_none());
}
