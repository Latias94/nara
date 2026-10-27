//! Actual C# child qualification through the public Application source-tool boundary.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    project_host::{
        EditorSourceToolCommand, EditorSourceToolDefinition, advanced::built_in_schema_providers,
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
        EditorSourceToolAction, EditorSourceToolActionOutcome, EditorSourceToolError,
        EditorSourceToolIntent, EditorSourceToolView,
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
        let view = application.current_frame().unwrap().source_tool().clone();
        if !view.has_pending_work() {
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

fn source_action(
    application: &mut EditorApplication,
    action: EditorSourceToolAction,
) -> EditorSourceToolActionOutcome {
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [EditorAction::SourceTool(action)]),
        Duration::ZERO,
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::SourceTool(outcome))] =
        submitted.actions()
    else {
        panic!("Application lost the source-tool action outcome: {submitted:?}");
    };
    outcome.clone()
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
    assert!(
        application
            .current_frame()
            .unwrap()
            .source_tool()
            .active
            .is_none()
    );
    let attempt = application.enable_source_tool(definition.clone()).unwrap();
    let ready = settle_tool(&mut application);
    let active = ready.active.as_ref().unwrap();
    assert_eq!(active.owner(), owner.owner_id());
    assert_eq!(active.component(), &ComponentTypeId::new(STATS));
    let facet = active.integer_property().unwrap();
    assert_eq!(
        facet.field(),
        &nara::reflect::ComponentFieldId::new("health")
    );
    assert_eq!(facet.parameter(), "bonus");
    assert_eq!(facet.label(), "Health bonus");
    assert_eq!(facet.initial(), 0);
    assert_eq!(ready.attempt.as_ref(), Some(&attempt));
    let activation = active.id().clone();
    let facet_target = EditorPropertyTarget {
        basis: basis.clone(),
        entity: enemy.clone(),
        component: ComponentTypeId::new(STATS),
        property: ComponentPropertyPath::new(ComponentFieldId::new("health")),
    };
    let EditorSourceToolActionOutcome::Requested(Ok(facet_request)) = source_action(
        &mut application,
        EditorSourceToolAction::IntegerProperty {
            activation: activation.clone(),
            target: facet_target.clone(),
            value: 25,
        },
    ) else {
        panic!("The admitted integer facet did not produce a request");
    };
    let ready = settle_tool(&mut application);
    assert!(ready.active.is_some() && ready.review.is_some());
    assert_eq!(ready.attempt.as_ref(), Some(&attempt));
    assert_eq!(ready.request.as_ref(), Some(&facet_request));
    let (facet_patch, facet_inverse) = {
        let review = application.source_tool_review().unwrap().unwrap();
        assert_eq!(review.request(), &facet_request);
        assert_eq!(review.title(), "Edit EnemyStats for ");
        (review.patch().clone(), review.inverse().clone())
    };
    assert_eq!(facet_patch.operations.len(), 2);
    assert_eq!(facet_inverse.operations.len(), 2);
    let mut facet_writes = BTreeMap::new();
    for operation in &facet_patch.operations {
        let ScenePatchOperation::SetField {
            entity,
            component,
            field,
            value,
            ..
        } = operation
        else {
            panic!("Unexpected facet operation: {operation:?}");
        };
        assert_eq!(entity, &enemy);
        assert_eq!(component, &ComponentTypeId::new(STATS));
        assert!(facet_writes.insert(field.as_str(), value.clone()).is_none());
    }
    assert_eq!(
        facet_writes,
        BTreeMap::from([
            ("armor", ComponentValue::I64(10)),
            ("health", ComponentValue::I64(125)),
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
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100)),
    ));
    let EditorSourceToolActionOutcome::Accepted(Ok(committed)) = source_action(
        &mut application,
        EditorSourceToolAction::Accept {
            request: facet_request.clone(),
        },
    ) else {
        panic!("The facet review was not accepted");
    };
    assert!(committed.report.applied, "{:?}", committed.report);
    assert_eq!(committed.report.document, Some(document));
    assert!(committed.created_entity.is_none());
    assert_eq!(
        committed
            .report
            .patch_report
            .as_ref()
            .unwrap()
            .inverse
            .as_ref(),
        Some(&facet_inverse)
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
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125)),
    );
    replay(&mut application, document, false);
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100)),
    );
    replay(&mut application, document, true);
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125)),
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    let observers = (
        application.current_frame().unwrap(),
        application.current_frame().unwrap(),
    );
    assert!(Arc::ptr_eq(&observers.0, &observers.1));
    assert_eq!(observers.0.source_tool(), observers.1.source_tool());
    let spawn_basis = application.document_query_basis(document).unwrap();
    let EditorSourceToolActionOutcome::Requested(Ok(spawn_request)) = source_action(
        &mut application,
        EditorSourceToolAction::Request {
            activation: activation.clone(),
            basis: spawn_basis.clone(),
            intent: EditorSourceToolIntent::Spawn {
                name: "Reinforcement".into(),
            },
            parameters: BTreeMap::from([("bonus".into(), ComponentValue::I64(25))]),
        },
    ) else {
        panic!("The actual C# Spawn request was not admitted");
    };
    let ready = settle_tool(&mut application);
    assert!(ready.review.is_some());
    let (spawn_patch, spawn_inverse, created) = {
        let review = application.source_tool_review().unwrap().unwrap();
        assert_eq!(review.request(), &spawn_request);
        assert_eq!(review.title(), "Spawn Reinforcement with EnemyStats");
        assert!(std::ptr::eq(
            review.patch(),
            application.source_tool_review().unwrap().unwrap().patch()
        ));
        (
            review.patch().clone(),
            review.inverse().clone(),
            review.created_entity().unwrap().clone(),
        )
    };
    assert_ne!(created, enemy);
    assert_eq!(spawn_patch.operations.len(), 6);
    assert!(matches!(&spawn_patch.operations[0],
        ScenePatchOperation::AddEntity { entity, placement }
            if entity.id == created && placement == &ScenePlacement::last(ScenePlacementOwner::Root)));
    assert!(!spawn_inverse.operations.is_empty());
    let observers = (
        application.current_frame().unwrap(),
        application.current_frame().unwrap(),
    );
    assert_eq!(observers.0.source_tool(), observers.1.source_tool());
    assert_eq!(
        observers.0.source_tool().review.as_ref().unwrap().request,
        spawn_request
    );
    let EditorSourceToolActionOutcome::Accepted(Ok(committed)) = source_action(
        &mut application,
        EditorSourceToolAction::Accept {
            request: spawn_request.clone(),
        },
    ) else {
        panic!("The actual C# Spawn review was not accepted");
    };
    assert_eq!(committed.created_entity.as_ref(), Some(&created));
    assert_eq!(
        committed
            .report
            .patch_report
            .as_ref()
            .unwrap()
            .inverse
            .as_ref(),
        Some(&spawn_inverse)
    );
    assert_eq!(
        read_property(&mut application, document, &created, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125))
    );
    replay(&mut application, document, false);
    assert!(matches!(
        read_property(&mut application, document, &created, STATS, "health"),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::EntityNotFound
        ))
    ));
    replay(&mut application, document, true);
    assert_eq!(
        read_property(&mut application, document, &created, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125))
    );
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 2);
    let unchanged = application.document_query_basis(document).unwrap();
    let deferred_request = EditorSourceToolAction::Request {
        activation: activation.clone(),
        basis: unchanged.clone(),
        intent: EditorSourceToolIntent::Edit {
            entity: enemy.clone(),
        },
        parameters: BTreeMap::from([("bonus".into(), ComponentValue::I64(99))]),
    };
    let deferred_property = EditorSourceToolAction::IntegerProperty {
        activation: activation.clone(),
        target: EditorPropertyTarget {
            basis: unchanged.clone(),
            ..facet_target
        },
        value: 99,
    };
    let deferred_disable = EditorSourceToolAction::Disable {
        activation: activation.clone(),
    };
    let replacement = application.enable_source_tool(definition).unwrap();
    let ready = settle_tool(&mut application);
    assert_eq!(ready.attempt.as_ref(), Some(&replacement));
    let next_activation = ready.active.as_ref().unwrap().id().clone();
    assert_ne!(next_activation, activation);
    for deferred in [deferred_request, deferred_property] {
        assert!(matches!(
            source_action(&mut application, deferred),
            EditorSourceToolActionOutcome::Requested(Err(EditorSourceToolError::StaleActivation))
        ));
    }
    assert!(matches!(
        source_action(&mut application, deferred_disable),
        EditorSourceToolActionOutcome::Disabled(Err(EditorSourceToolError::StaleActivation))
    ));
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        unchanged
    );
    let EditorSourceToolActionOutcome::Requested(Ok(revoked)) = source_action(
        &mut application,
        EditorSourceToolAction::IntegerProperty {
            activation: next_activation.clone(),
            target: EditorPropertyTarget {
                basis: unchanged.clone(),
                entity: enemy.clone(),
                component: ComponentTypeId::new(STATS),
                property: ComponentPropertyPath::new(ComponentFieldId::new("health")),
            },
            value: 50,
        },
    ) else {
        panic!("The second facet request was not admitted");
    };
    assert!(settle_tool(&mut application).review.is_some());
    assert!(matches!(
        source_action(
            &mut application,
            EditorSourceToolAction::Disable {
                activation: next_activation,
            }
        ),
        EditorSourceToolActionOutcome::Disabled(Ok(()))
    ));
    assert!(
        application
            .current_frame()
            .unwrap()
            .source_tool()
            .active
            .is_none()
    );
    assert!(matches!(
        source_action(
            &mut application,
            EditorSourceToolAction::Accept { request: revoked }
        ),
        EditorSourceToolActionOutcome::Accepted(Err(EditorSourceToolError::Disabled))
    ));
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        unchanged
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    replay(&mut application, document, false);
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 1);
    assert_eq!(tab.history.redo_depth, 1);
    replay(&mut application, document, false);
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 2);
    assert!(!tab.dirty);
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100))
    );
    close(&mut application);
}
