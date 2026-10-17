#![cfg(any(target_os = "windows", target_os = "linux"))]

use std::{path::Path, time::Duration};

use nara::{
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{AuthoringFieldEdit, SceneEntityId},
};
use nara_editor::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, ImguiEditorFrontend,
    ImguiEditorPanelContext, workspace::EditorPanelTarget,
};
use nara_editor_panel_module_consumer::{
    PANEL_KIND, clear_selection, install, install_imgui, selected_count,
};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
    EditorNavigationAction, EditorWorkspaceIntentAction,
};

fn clear_selected_from_companion(
    _ui: &dear_imgui_rs::Ui,
    context: &mut ImguiEditorPanelContext<'_, '_>,
) {
    assert_eq!(context.model().instance().kind(), PANEL_KIND);
    assert_eq!(selected_count(context.model()), 1);
    context.emit(clear_selection(context.model()).unwrap());
}

#[test]
fn a_separately_compiled_imgui_companion_reopens_without_owning_document_selection_or_history() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../generic-editor/known-unbound");
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
    let scene = selected.workspace().active_scene.as_ref().unwrap();
    let selection = scene.selection.clone();
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
                        ComponentValue::String("ImGui panel lifetime proof".to_owned()),
                    )
                    .unwrap(),
                },
            )],
        ),
        Duration::ZERO,
    );
    let before = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    let target = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap()
        .target()
        .clone();
    let mut frontend = ImguiEditorFrontend::new();
    install_imgui(&mut frontend);
    frontend.register_panel_painter(PANEL_KIND, clear_selected_from_companion);
    let mut context = dear_imgui_rs::Context::create();
    context
        .set_ini_filename::<std::path::PathBuf>(None)
        .unwrap();
    context.io_mut().set_display_size([1600.0, 1000.0]);
    context.io_mut().set_delta_time(1.0 / 60.0);
    context
        .io_mut()
        .set_config_flags(dear_imgui_rs::ConfigFlags::DOCKING_ENABLE);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    let panel_closed = application
        .panel_workspace_mut()
        .layout_mut()
        .close(instance_id);
    let frame = context.begin_frame();
    let batch = frontend
        .show(frame.ui(), &mut application)
        .unwrap()
        .unwrap();
    drop(frame);
    frontend.submit_and_drive(&mut application, batch, Duration::ZERO);
    let closed_frame = application.current_frame().unwrap();
    let closed_panel = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap();
    let panel_was_hidden = !closed_panel.is_open() && !closed_panel.is_visible();
    let closed_document_unchanged = closed_frame.workspace().document(document) == Some(&before);
    let closed_selection_unchanged = closed_frame
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        == selection;
    let panel_reopened = application
        .panel_workspace_mut()
        .layout_mut()
        .reopen(instance_id);
    let reopened_panel = application
        .panel_workspace()
        .layout()
        .instance(instance_id)
        .unwrap();
    let panel_was_reopened = reopened_panel.is_open()
        && reopened_panel.is_visible()
        && reopened_panel.target() == &target;
    let frame = context.begin_frame();
    let batch = frontend
        .show(frame.ui(), &mut application)
        .unwrap()
        .unwrap();
    drop(frame);
    let before_dispatch = application.current_frame().unwrap();
    let painting_preserved_selection = before_dispatch
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        == selection;
    frontend.submit_and_drive(&mut application, batch, Duration::ZERO);
    let after = application.current_frame().unwrap();
    let selection_cleared = after
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .is_empty();
    let duplicate = application
        .panel_workspace_mut()
        .open(PANEL_KIND, EditorPanelTarget::ActiveDocument)
        .unwrap();
    let document_unchanged = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        == Some(&before);
    let duplicate_target_unchanged = application
        .panel_workspace()
        .layout()
        .instance(duplicate)
        .unwrap()
        .target()
        == &target;
    let mut closed = application.request_exit().exit_ready();
    if !closed && let Some(frame) = application.current_frame() {
        closed = application
            .submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(
                        EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Discard),
                    )],
                ),
                Duration::ZERO,
            )
            .exit_ready();
    }
    for _ in 0..128 {
        if closed {
            break;
        }
        let Some(frame) = application.current_frame() else {
            break;
        };
        closed = application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
            .exit_ready();
        if !closed {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(closed);
    assert!(before.dirty);
    assert_eq!(before.history.undo_depth, 1);
    assert!(panel_closed && panel_was_hidden);
    assert!(closed_document_unchanged && closed_selection_unchanged);
    assert!(panel_reopened && panel_was_reopened);
    assert!(painting_preserved_selection);
    assert!(selection_cleared);
    assert!(document_unchanged);
    assert_ne!(instance_id, duplicate);
    assert!(duplicate_target_unchanged);
}
