use super::*;
use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    tooling::{EditorNavigationAction, EditorPersistenceAction, EditorPersistenceView},
};

fn source_state(interaction: &Interaction) -> (Vec<String>, usize, bool) {
    let frame = interaction.frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    (
        scene
            .editor
            .inspector
            .entities
            .iter()
            .map(|entity| entity.id.as_str().to_owned())
            .collect(),
        scene.editor.inspector.history.undo_depth,
        scene.dirty,
    )
}

#[test]
fn queued_hierarchy_creation_refuses_a_selection_change_without_source_revision_change() {
    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    interaction.click_hierarchy("Select preserved");
    let before = interaction.frame();
    let scene = before.workspace().active_scene.as_ref().unwrap();
    let state = source_state(&interaction);
    interaction.queue_hierarchy_click("+ Child");
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            before.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(SceneEntityId::new("prefab-anchor").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        scene.revision
    );
    interaction.render();
    assert!(
        interaction.actions.is_empty(),
        "a displayed Child cannot retarget after selection changes"
    );
    assert_eq!(source_state(&interaction), state);
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
            .top_entity(),
        Some(&SceneEntityId::new("prefab-anchor").unwrap())
    );
}

#[test]
fn superseded_hierarchy_submission_keeps_source_and_allows_an_explicit_retry() {
    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    let state = source_state(&interaction);
    interaction.queue_hierarchy_click("+ Root");
    let unpublished = interaction.draw().unwrap();
    assert_eq!(source_state(&interaction), state);
    drop(unpublished);
    interaction.render();
    assert_eq!(source_state(&interaction), state);
    interaction.click_hierarchy("+ Root");
    let after = source_state(&interaction);
    assert_eq!(after.0.len(), state.0.len() + 1);
    assert_eq!(after.1, state.1 + 1);
    assert!(after.2);
}

#[test]
fn queued_hierarchy_action_refuses_the_actual_dirty_close_decision_boundary() {
    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    interaction.click_hierarchy("+ Root");
    let state = source_state(&interaction);
    interaction.queue_hierarchy_click("+ Child");
    assert!(!interaction.application.request_exit().exit_ready());
    assert!(
        interaction
            .frame()
            .workspace_intent()
            .decision_prompt()
            .is_some()
    );
    interaction.render();
    assert!(interaction.actions.is_empty());
    assert_eq!(source_state(&interaction), state);
    assert!(
        interaction
            .frame()
            .workspace_intent()
            .decision_prompt()
            .is_some()
    );
}

fn reopen_without_paint(interaction: &mut Interaction) {
    let frame = interaction.frame();
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: frame.reopen_target(),
            })],
        ),
        Duration::ZERO,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while interaction.frame().persistence().operation().is_some() && Instant::now() < deadline {
        interaction.application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let frame = interaction.frame();
    assert!(matches!(
        frame.persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    let EditorPersistenceView::Terminal { attempt, .. } = frame.persistence() else {
        unreachable!()
    };
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )],
        ),
        Duration::ZERO,
    );
}

#[test]
fn same_document_specialized_projection_retires_an_unqueued_source_action_and_recovers() {
    let project = super::super::ui_interaction_tests::UiProject::new();
    let source = project.0.join("prefabs/ui.prefab.json");
    let ui_bytes = fs::read(&source).unwrap();
    let mut ordinary: serde_json::Value = serde_json::from_slice(&ui_bytes).unwrap();
    for entity in ordinary["payload"]["entities"].as_array_mut().unwrap() {
        entity["components"].as_object_mut().unwrap().clear();
    }
    let ordinary_bytes = serde_json::to_vec_pretty(&ordinary).unwrap();
    fs::write(&source, &ordinary_bytes).unwrap();
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project.0).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    application.native_shell_ready();
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let anchor = SceneEntityId::new("ui-anchor").unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: anchor,
                }),
            ],
        ),
        Duration::ZERO,
    );
    let mut interaction = Interaction::from_application(application);
    assert!(
        interaction
            .application
            .current_ui_authoring_frame()
            .is_none()
    );
    let document = interaction.frame().workspace().active_document.unwrap();
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .document(document)
            .unwrap()
            .kind,
        EditorDocumentKind::Prefab
    );
    let panel = interaction.hierarchy_panel();
    let incarnation = Arc::clone(&interaction.frontend.presentation.widgets[&panel].incarnation);
    let before = source_state(&interaction);
    interaction.queue_hierarchy_click("+ Root");
    assert_eq!(
        interaction.frontend.presentation.widgets[&panel]
            .hierarchy_pending
            .len(),
        1
    );

    fs::write(&source, &ui_bytes).unwrap();
    reopen_without_paint(&mut interaction);
    assert_eq!(
        interaction.frame().workspace().active_document,
        Some(document)
    );
    assert!(
        interaction
            .application
            .current_ui_authoring_frame()
            .is_some()
    );
    interaction.render();
    let widgets = &interaction.frontend.presentation.widgets[&panel];
    assert!(Arc::ptr_eq(&incarnation, &widgets.incarnation));
    assert!(widgets.hierarchy_pending.is_empty());
    assert!(
        widgets
            .owned_hierarchy
            .as_ref()
            .is_some_and(|owner| owner.is_ui())
    );

    interaction.click_hierarchy("Add UI node");
    interaction.queue_hierarchy_click("Apply structure change");
    assert_eq!(
        interaction.frontend.presentation.widgets[&panel]
            .hierarchy_pending
            .len(),
        1
    );

    fs::write(&source, &ordinary_bytes).unwrap();
    reopen_without_paint(&mut interaction);
    assert_eq!(
        interaction.frame().workspace().active_document,
        Some(document)
    );
    assert!(
        interaction
            .application
            .current_ui_authoring_frame()
            .is_none()
    );
    interaction.render();
    assert_eq!(source_state(&interaction), before);
    interaction.click_hierarchy("+ Root");
    let after = source_state(&interaction);
    assert_eq!(after.0.len(), before.0.len() + 1);
    assert_eq!(after.1, 1);
    assert!(after.2);
}

fn ui_interaction(project: &super::super::ui_interaction_tests::UiProject) -> Interaction {
    let mut application = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(&project.0).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    application.native_shell_ready();
    super::super::ui_interaction_tests::setup_ui_prefab_source(&mut application);
    Interaction::from_application(application)
}

#[test]
fn queued_ui_structure_refuses_same_revision_selection_change_before_queue_admission() {
    let project = super::super::ui_interaction_tests::UiProject::new();
    let mut interaction = ui_interaction(&project);
    interaction.click_hierarchy("Select UI image-node");
    interaction.click_hierarchy("Add UI node");
    let before = interaction.frame();
    let state = source_state(&interaction);
    let scene = before.workspace().active_scene.as_ref().unwrap();
    interaction.queue_hierarchy_click("Apply structure change");
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            before.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(SceneEntityId::new("ui-root").unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        scene.revision
    );
    interaction.render();
    assert!(interaction.actions.is_empty());
    assert_eq!(source_state(&interaction), state);
    assert_eq!(
        interaction
            .application
            .current_ui_authoring_frame()
            .unwrap()
            .selection()
            .top_entity()
            .unwrap()
            .as_str(),
        "ui-root"
    );
}

#[test]
fn superseded_ui_structure_submission_preserves_an_explicit_retry_and_one_history_entry() {
    let project = super::super::ui_interaction_tests::UiProject::new();
    let mut interaction = ui_interaction(&project);
    let state = source_state(&interaction);
    interaction.click_hierarchy("Add UI node");
    interaction.queue_hierarchy_click("Apply structure change");
    drop(interaction.draw().unwrap());
    interaction.render();
    assert_eq!(source_state(&interaction), state);
    interaction.click_hierarchy("Apply structure change");
    let after = source_state(&interaction);
    assert_eq!(after.0.len(), state.0.len() + 1);
    assert_eq!(after.1, state.1 + 1);
    assert!(after.2);
}

#[test]
fn queued_ui_structure_refuses_an_actual_dirty_close_decision() {
    let project = super::super::ui_interaction_tests::UiProject::new();
    let mut interaction = ui_interaction(&project);
    interaction.click_hierarchy("Add UI node");
    interaction.click_hierarchy("Apply structure change");
    let state = source_state(&interaction);
    interaction.click_hierarchy("Add UI node");
    interaction.queue_hierarchy_click("Apply structure change");
    assert!(!interaction.application.request_exit().exit_ready());
    interaction.render();
    assert!(interaction.actions.is_empty());
    assert_eq!(source_state(&interaction), state);
    assert!(
        interaction
            .frame()
            .workspace_intent()
            .decision_prompt()
            .is_some()
    );
}

#[test]
fn ui_structure_result_keeps_its_original_slot_when_an_earlier_panel_is_retired() {
    use crate::workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement, EditorPanelTarget,
    };
    use std::{cell::Cell, rc::Rc};
    struct EarlierPanel(Rc<Cell<bool>>);
    impl super::super::ImguiEditorPanel for EarlierPanel {
        fn show(
            &mut self,
            _: &dear_imgui_rs::Ui,
            context: &mut super::super::ImguiEditorPanelContext<'_, '_>,
        ) {
            if !self.0.replace(false) {
                return;
            }
            let model = context.model();
            let document = model.document().unwrap().document;
            let entity = model
                .selection()
                .and_then(|selection| selection.top_entity())
                .cloned();
            context
                .submit(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity { document, entity },
                ))
                .unwrap();
        }
    }
    let project = super::super::ui_interaction_tests::UiProject::new();
    let mut interaction = ui_interaction(&project);
    let original = interaction.hierarchy_panel();
    interaction
        .application
        .panel_workspace_mut()
        .layout_mut()
        .close(original);
    let fire = Rc::new(Cell::new(false));
    let factory = Rc::clone(&fire);
    interaction
        .application
        .panel_workspace_mut()
        .register(EditorPanelDescriptor::new(
            "test.ui-earlier",
            "Earlier",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Floating,
            true,
        ))
        .unwrap();
    interaction.frontend.register_panel_factory(
        "test.ui-earlier".into(),
        Box::new(move || Box::new(EarlierPanel(Rc::clone(&factory)))),
    );
    let earlier = interaction
        .application
        .panel_workspace_mut()
        .open("test.ui-earlier", EditorPanelTarget::ActiveDocument)
        .unwrap();
    let hierarchy = interaction
        .application
        .panel_workspace_mut()
        .open("nara.hierarchy", EditorPanelTarget::ActiveDocument)
        .unwrap();
    let mut preferences: serde_json::Value = serde_json::from_slice(
        &interaction
            .application
            .panel_workspace()
            .layout()
            .to_json()
            .unwrap(),
    )
    .unwrap();
    preferences["panels"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|panel| panel["kind"].as_str() != Some("test.ui-earlier"));
    interaction.application.panel_workspace_mut().restore(
        crate::workspace::EditorWorkspaceLayout::from_json(
            &serde_json::to_vec(&preferences).unwrap(),
        )
        .unwrap(),
        "dear-imgui",
        1,
        &[],
    );
    for _ in 0..4 {
        interaction.render();
    }
    interaction.click_hierarchy("Add UI node");
    let state = source_state(&interaction);
    interaction.queue_hierarchy_click("Apply structure change");
    fire.set(true);
    let candidate = interaction.draw().unwrap();
    let pending = interaction.frontend.pending.as_ref().unwrap();
    assert_eq!(
        pending
            .submissions
            .iter()
            .find(|submission| submission.origin.instance == hierarchy)
            .unwrap()
            .slot,
        1
    );
    interaction
        .application
        .panel_workspace_mut()
        .layout_mut()
        .set_visible(earlier, false);
    let dispatch = interaction
        .frontend
        .dispatch_submission(&mut interaction.application, candidate)
        .unwrap();
    assert_eq!(dispatch.original_slots(), &[None, Some(0)]);
    interaction
        .application
        .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
    interaction
        .frontend
        .finish_dispatch(&interaction.application, &dispatch);
    interaction.render();
    let after = source_state(&interaction);
    assert_eq!(after.0.len(), state.0.len() + 1);
    assert_eq!(after.1, state.1 + 1);
    interaction.click_hierarchy("Add UI node");
    interaction.click_hierarchy("Apply structure change");
    assert_eq!(
        source_state(&interaction).1,
        state.1 + 2,
        "the exact completion must release the original pending operation"
    );
}
