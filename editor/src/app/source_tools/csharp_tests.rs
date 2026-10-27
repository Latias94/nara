//! Actual C# child qualification through the public Application source-tool boundary.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara::{
    project_host::{
        EditorSourceToolCommand, EditorSourceToolDefinition, EditorSourceToolError,
        EditorSourceToolIntent, EditorSourceToolView, advanced::built_in_schema_providers,
    },
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentPropertyPath,
        ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
    },
    scene::{
        AuthoringReadError, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
        ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId,
        EditorDocumentMutation, EditorDocumentQueryRejection, EditorPropertyRead,
        EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    },
};

use crate::app::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationState,
    persistence_tests::{PersistenceFixture, close},
};

const STATS: &str = "trial.enemy.EnemyStats";
const NAME: &str = "nara.scene.Name";
const TIMEOUT: Duration = Duration::from_secs(30);

fn settle_tool(application: &mut EditorApplication) -> EditorSourceToolView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.source_tool_view().unwrap();
        if !view.probing && !view.working && !view.retiring {
            assert!(
                view.attempt_error.is_none() && view.request_error.is_none(),
                "{view:?}"
            );
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Actual C# child did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn read_property(
    application: &mut EditorApplication,
    document: EditorDocumentId,
    entity: &SceneEntityId,
    component: &str,
    field: &str,
) -> Result<EditorPropertyValue, EditorDocumentQueryRejection> {
    let target = EditorPropertyTarget {
        basis: application.document_query_basis(document).unwrap(),
        entity: entity.clone(),
        component: ComponentTypeId::new(component),
        property: ComponentPropertyPath::new(ComponentFieldId::new(field)),
    };
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ReadDocumentProperty(EditorPropertyRead {
                target,
                limits: EditorPropertyReadLimits::new(4096, 64, 4096).unwrap(),
            })],
        ),
        Duration::ZERO,
    );
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentProperty(result)),
    ] = submitted.actions()
    else {
        panic!("Application lost the property read outcome: {submitted:?}");
    };
    result
        .as_ref()
        .map(|observation| observation.value.clone())
        .map_err(Clone::clone)
}

fn replay(application: &mut EditorApplication, document: EditorDocumentId, redo: bool) {
    let frame = application.current_frame().unwrap();
    let base_revision = application.document_query_basis(document).unwrap().revision;
    let mutation = if redo {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    };
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [EditorAction::Mutate(mutation)]),
        Duration::ZERO,
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] =
        submitted.actions()
    else {
        panic!("Application lost the history outcome: {submitted:?}");
    };
    assert!(report.applied, "{report:?}");
    assert_eq!(report.document, Some(document));
}

#[test]
#[ignore = "run behavior-languages/run.py for actual .NET tool artifacts"]
fn csharp_source_tool_application_preserves_review_and_exact_commit_result() {
    let directory = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_V1_DIR")
            .expect("Runner supplies the actual C# tool output directory"),
    );
    let executable = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_DOTNET")
            .expect("Runner supplies the resolved dotnet executable"),
    );
    let catalog = ComponentSchemaCatalog::from_json_bytes(
        &fs::read(directory.join("enemy-stats.catalog.json")).unwrap(),
    )
    .unwrap();
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("trial.enemy-stats").unwrap(),
        catalog,
        None,
    )
    .unwrap();
    let component = ComponentTypeId::new(STATS);
    let native =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap();
    let bundle = AuthoringSchemaBundle::new(
        native
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .chain([AuthoringSchemaBundleOwner::from(&owner)]),
    )
    .unwrap();
    let name_owner = bundle
        .owner_for_component(&ComponentTypeId::new(NAME))
        .unwrap();
    let enemy = SceneEntityId::new("enemy").unwrap();
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(enemy.clone())
        .with_component(
            ComponentTypeId::new(NAME),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Guard".into()),
            )
            .with_owner(name_owner.owner_id().clone()),
        )
        .with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                owner
                    .schema(&component)
                    .unwrap()
                    .authoring_initial_value()
                    .unwrap()
                    .clone(),
            )
            .with_owner(owner.owner_id().clone()),
        )]);
    let fixture = PersistenceFixture::new();
    fs::write(
        fixture.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    let scene_path = fixture.root.join("worlds/entry.scene.json");
    fs::write(&scene_path, source.to_json_string().unwrap()).unwrap();
    let original_bytes = fs::read(&scene_path).unwrap();
    let command = EditorSourceToolCommand::new(
        executable,
        vec![directory.join("EnemyStatsTool.dll").into_os_string()],
        directory,
        TIMEOUT,
    )
    .unwrap();
    let definition = EditorSourceToolDefinition::new(command, owner.clone(), component).unwrap();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    let basis = application.document_query_basis(document).unwrap();
    assert_eq!(
        basis
            .schema
            .as_ref()
            .unwrap()
            .component_schema_owner(&ComponentTypeId::new(STATS)),
        Some(owner.owner_id()),
    );
    assert!(!application.source_tool_view().unwrap().enabled);
    let attempt = application.enable_source_tool(definition).unwrap();
    let ready = settle_tool(&mut application);
    assert!(ready.enabled);
    assert_eq!(ready.attempt.as_ref(), Some(&attempt));
    let request = application
        .request_source_tool(
            &basis,
            EditorSourceToolIntent::Spawn {
                name: "Reinforcement".into(),
            },
            BTreeMap::from([("bonus".into(), ComponentValue::I64(25))]),
        )
        .unwrap();
    let ready = settle_tool(&mut application);
    assert!(ready.review_ready);
    assert_eq!(ready.attempt.as_ref(), Some(&attempt));
    assert_eq!(ready.request.as_ref(), Some(&request));
    let (patch, inverse, created) = {
        let review = application.source_tool_review().unwrap().unwrap();
        assert_eq!(review.request(), &request);
        assert_eq!(review.title(), "Spawn Reinforcement with EnemyStats");
        (
            review.patch().clone(),
            review.inverse().clone(),
            review.created_entity().unwrap().clone(),
        )
    };
    assert_ne!(created, enemy);
    assert!(!inverse.operations.is_empty());
    assert_eq!(patch.operations.len(), 6);
    let mut writes = BTreeMap::new();
    let mut added_components = Vec::new();
    for operation in &patch.operations {
        match operation {
            ScenePatchOperation::AddEntity { entity, placement } => {
                assert_eq!(entity.id, created);
                assert_eq!(placement, &ScenePlacement::last(ScenePlacementOwner::Root));
            }
            ScenePatchOperation::AddComponent {
                entity,
                component,
                value,
            } => {
                assert_eq!(entity, &created);
                let expected_owner = bundle.owner_for_component(component).unwrap();
                assert_eq!(value.owner(), Some(expected_owner.owner_id()));
                added_components.push(component.as_str());
            }
            ScenePatchOperation::SetField {
                entity,
                component,
                field,
                value,
                ..
            } => {
                assert_eq!(entity, &created);
                assert!(
                    writes
                        .insert((component.as_str(), field.as_str()), value.clone())
                        .is_none()
                );
            }
            other => panic!("Unexpected reviewed operation: {other:?}"),
        }
    }
    added_components.sort_unstable();
    assert_eq!(added_components, vec![NAME, STATS]);
    assert_eq!(
        writes,
        BTreeMap::from([
            (
                (NAME, "value"),
                ComponentValue::String("Reinforcement".into())
            ),
            ((STATS, "armor"), ComponentValue::I64(10)),
            ((STATS, "health"), ComponentValue::I64(125)),
        ])
    );
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    let frame = application.current_frame().unwrap();
    assert_eq!(
        frame
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        0
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    assert!(matches!(
        read_property(&mut application, document, &created, STATS, "health"),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::EntityNotFound
        )),
    ));
    let submitted = application.accept_source_tool_review(&request, Duration::ZERO);
    assert!(submitted.update().actions_observed());
    let committed = submitted.result().as_ref().unwrap();
    assert!(committed.report.applied, "{:?}", committed.report);
    assert_eq!(committed.report.document, Some(document));
    assert_eq!(committed.created_entity.as_ref(), Some(&created));
    assert_eq!(
        committed
            .report
            .patch_report
            .as_ref()
            .unwrap()
            .inverse
            .as_ref(),
        Some(&inverse)
    );
    assert_eq!(
        submitted.into_result().unwrap().created_entity.as_ref(),
        Some(&created)
    );
    assert!(application.source_tool_review().unwrap().is_none());
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1,
    );
    for (entity, health) in [(&enemy, 100), (&created, 125)] {
        for (field, value) in [("health", health), ("armor", 10)] {
            assert_eq!(
                read_property(&mut application, document, entity, STATS, field).unwrap(),
                EditorPropertyValue::Present(ComponentValue::I64(value)),
            );
        }
    }
    replay(&mut application, document, false);
    assert!(matches!(
        read_property(&mut application, document, &created, STATS, "health"),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::EntityNotFound
        )),
    ));
    replay(&mut application, document, true);
    assert_eq!(
        read_property(&mut application, document, &created, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125)),
    );
    assert_eq!(
        read_property(&mut application, document, &created, NAME, "value").unwrap(),
        EditorPropertyValue::Present(ComponentValue::String("Reinforcement".into())),
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    replay(&mut application, document, false);
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 1);
    assert!(!tab.dirty);
    let unchanged = application.document_query_basis(document).unwrap();
    let revoked = application
        .request_source_tool(
            &unchanged,
            EditorSourceToolIntent::Edit {
                entity: enemy.clone(),
            },
            BTreeMap::from([("bonus".into(), ComponentValue::I64(50))]),
        )
        .unwrap();
    let ready = settle_tool(&mut application);
    assert!(ready.review_ready);
    assert_eq!(ready.request.as_ref(), Some(&revoked));
    assert_eq!(
        application.source_tool_review().unwrap().unwrap().request(),
        &revoked
    );
    application.disable_source_tool().unwrap();
    assert!(!settle_tool(&mut application).enabled);
    assert!(application.source_tool_review().unwrap().is_none());
    assert!(matches!(
        application
            .accept_source_tool_review(&revoked, Duration::ZERO)
            .into_result(),
        Err(EditorSourceToolError::Disabled),
    ));
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        unchanged
    );
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100)),
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    close(&mut application);
}
