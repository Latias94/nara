use super::*;
use nara::{
    ProductRecipe,
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::ComponentTypeId,
    scene::{SceneEntityId, ScenePlacement, ScenePlacementOwner},
    tooling::{
        EditorDocumentMutation, EditorPersistenceAction, EditorPersistenceResult,
        EditorReopenTarget, SceneCompositionCommand, SceneCompositionEntityKind,
    },
};

pub(super) fn arrange_save_reopen_and_check(
    project: &CleanRoomSupervisedPlayProject,
    application: &mut EditorApplication,
) {
    let source = project.root().join("scenes/startup.scene.json");
    let initial_bytes = std::fs::read(&source).unwrap();
    let library = std::fs::read(project.root().join("src/lib.rs")).unwrap();
    let first = create(application, ScenePlacement::last(ScenePlacementOwner::Root));
    add_transform(application, first.clone());
    let child = create(
        application,
        ScenePlacement::last(ScenePlacementOwner::Parent {
            parent: first.clone(),
        }),
    );
    add_transform(application, child.clone());
    let second = create(application, ScenePlacement::last(ScenePlacementOwner::Root));
    add_transform(application, second.clone());
    compose(
        application,
        SceneCompositionCommand::MoveEntity {
            entity: child.clone(),
            to: ScenePlacement::last(ScenePlacementOwner::Parent {
                parent: second.clone(),
            }),
        },
    );
    compose(
        application,
        SceneCompositionCommand::MoveEntity {
            entity: second.clone(),
            to: ScenePlacement::first(ScenePlacementOwner::Root),
        },
    );
    let arranged = tree(application);
    assert_eq!(arranged[0].0, second.clone());
    assert!(arranged.contains(&(child.clone(), Some(second.clone()))));
    compose(
        application,
        SceneCompositionCommand::RemoveEntity {
            entity: first.clone(),
        },
    );
    assert!(!tree(application).iter().any(|(entity, _)| entity == &first));
    history(application, false);
    assert_eq!(tree(application), arranged);
    history(application, true);
    let accepted = tree(application);
    assert!(!accepted.iter().any(|(entity, _)| entity == &first));
    assert_eq!(
        std::fs::read(&source).unwrap(),
        initial_bytes,
        "working structure must not implicitly write source"
    );

    let current = current_frame(application);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let document = scene.document;
    let revision = scene.revision;
    dispatch(
        application,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        }),
    );
    settle(application);
    assert!(
        matches!(current_frame(application).persistence().result(), Some(EditorPersistenceResult::Saved {
        document: actual, revision: saved, ..
    }) if actual == document && saved == revision)
    );
    let saved = std::fs::read(&source).unwrap();
    assert_ne!(saved, initial_bytes);
    let attempt = current_frame(application)
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    dispatch(
        application,
        EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: revision,
            },
        }),
    );
    settle(application);
    assert!(matches!(
        current_frame(application).persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    assert_eq!(tree(application), accepted);
    let reopened = current_frame(application);
    let scene = reopened.workspace().active_scene.as_ref().unwrap();
    assert!(!scene.dirty);
    assert_eq!(scene.editor.inspector.history.undo_depth, 0);
    assert_eq!(
        application
            .active_authoring_unavailable_record_manifest()
            .unwrap()
            .total_count(),
        0
    );
    let attempt = current_frame(application)
        .persistence()
        .attempt()
        .expect("the test observed a terminal persistence result");
    dispatch(
        application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt }),
    );
    check_project(
        ProjectCheckRequest::new(project.capability(), ProductRecipe::new())
            .with_profile("desktop"),
    )
    .expect("the ordinary arranged source must pass strict native runtime admission");
    assert_eq!(
        std::fs::read(&source).unwrap(),
        saved,
        "strict Check must not rewrite authored structure"
    );
    assert_eq!(
        std::fs::read(project.root().join("src/lib.rs")).unwrap(),
        library
    );
}

fn create(application: &mut EditorApplication, placement: ScenePlacement) -> SceneEntityId {
    let before = tree(application).len();
    compose(
        application,
        SceneCompositionCommand::CreateEntity {
            kind: SceneCompositionEntityKind::Empty,
            placement,
        },
    );
    assert_eq!(tree(application).len(), before + 1);
    current_frame(application)
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .selection
        .top_entity()
        .unwrap()
        .clone()
}

fn add_transform(application: &mut EditorApplication, entity: SceneEntityId) {
    compose(
        application,
        SceneCompositionCommand::AddComponent {
            entity,
            component: ComponentTypeId::new("nara.transform.Transform2d"),
        },
    );
}

fn compose(application: &mut EditorApplication, command: SceneCompositionCommand) {
    let current = current_frame(application);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let before = scene.revision;
    dispatch(
        application,
        EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            document: scene.document,
            base_revision: before,
            command,
        }),
    );
    assert_ne!(
        current_frame(application)
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        before,
        "ordinary structural action must commit through the public Editor application"
    );
}

fn history(application: &mut EditorApplication, redo: bool) {
    let current = current_frame(application);
    let scene = current.workspace().active_scene.as_ref().unwrap();
    dispatch(
        application,
        EditorAction::Mutate(if redo {
            EditorDocumentMutation::Redo {
                document: scene.document,
                base_revision: scene.revision,
            }
        } else {
            EditorDocumentMutation::Undo {
                document: scene.document,
                base_revision: scene.revision,
            }
        }),
    );
}

fn tree(application: &EditorApplication) -> Vec<(SceneEntityId, Option<SceneEntityId>)> {
    current_frame(application)
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .entities
        .iter()
        .map(|row| (row.id.clone(), row.parent.clone()))
        .collect()
}

fn dispatch(application: &mut EditorApplication, action: EditorAction) {
    let current = current_frame(application);
    assert!(
        application
            .submit_and_drive(
                EditorActionBatch::new(current.revision(), [action]),
                Duration::ZERO
            )
            .update()
            .actions_observed()
    );
}

fn settle(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if current_frame(application)
            .persistence()
            .operation()
            .is_none()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "ordinary starter persistence must complete"
        );
        let current = current_frame(application);
        application.submit_and_drive(
            EditorActionBatch::new(current.revision(), []),
            Duration::ZERO,
        );
        thread::sleep(Duration::from_millis(1));
    }
}
