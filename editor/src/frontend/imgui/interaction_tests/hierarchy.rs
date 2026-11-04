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
    assert!(widgets.owned_hierarchy.is_none());

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
