#![cfg(feature = "serde")]

use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_ecs::World;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericBound, ComponentNumericInterval,
    ComponentRegistry, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape, EntityReference,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentReadiness, AuthoringDocumentSource, AuthoringFieldEdit,
    AuthoringRecordReadiness, AuthoringSessionAuthorityError, AuthoringSessionReplacementError,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, PrefabInstance,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SceneFormatError, ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    UnavailableRecordManifestEntry, UnavailableValuePathSegment, spawn_prefab, spawn_scene,
};

const OWNER: &str = "nara.scene.hierarchy.components";

#[test]
fn data_only_interval_rejection_retains_engine_code_and_root_without_nested_map_keys() {
    let canary = "qvM7k2p8Nx4J";
    let component = component_id("demo.WeightedCooldown");
    let owner = persistent_owner("demo.cooldown");
    let schema = ComponentSchema::new(
        component.clone(),
        "Weighted cooldown",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("weights"),
        "Weights",
        ComponentFieldPath::from_fields(["weights"]),
        ComponentValueShape::map(
            ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
                ComponentNumericInterval::U64 {
                    min: None,
                    max: Some(ComponentNumericBound::Included(10)),
                },
            ),
        ),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        },
        None,
    )])
    .unwrap();
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
        &bundle.owners()[0],
        &Default::default(),
    )
    .unwrap();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("root"))
        .with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "weights",
                    ComponentValue::map([(canary, ComponentValue::U64(11))]),
                )]),
            )
            .with_owner(owner),
        )]);
    let rejection =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/cooldown.scene.json"), &bundle, &[adapter])
            .unwrap_err();
    let diagnostic = rejection
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "nara.reflect.numeric-range")
        .unwrap();
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "component-field-id"
                && field.value() == nara_diagnostic::DiagnosticValueRef::Identifier("weights"))
    );
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "scene-context-code"
                && field.value()
                    == nara_diagnostic::DiagnosticValueRef::Identifier(
                        "scene.authoring-component-value-invalid"
                    ))
    );
    assert!(!format!("{:?}", rejection.diagnostics()).contains(canary));
}

#[test]
fn generic_insertion_tracks_unknown_placement_and_undo_restores_it_exactly() {
    let (bundle, adapter, document, _) = cross_component_authoring_fixture();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/placement.scene.json"), &bundle, &[adapter])
            .unwrap()
            .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let before = session.document().clone();
    let prepend =
        session.apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(scene_id("prepended")),
            placement: ScenePlacement::first(ScenePlacementOwner::Root),
        }]));
    assert!(prepend.applied, "{:?}", prepend.diagnostics);
    assert_ne!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &before);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    let append =
        session.apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(scene_id("appended")),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]));
    assert!(append.applied, "{:?}", append.diagnostics);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &before);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn generic_composition_preserves_unavailable_records_and_round_trips_history() {
    let (bundle, adapter, document, _) = cross_component_authoring_fixture();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/composition.scene.json"), &bundle, &[adapter])
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let created = scene_id("created");
    let create =
        session.apply_authoring_patch(&ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(created.clone()),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]));
    assert!(create.applied, "{:?}", create.diagnostics);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

    let add = session.apply_authoring_patch(&ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: created.clone(),
            component: component_id("nara.scene.Name"),
            value: SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Entity".to_owned()),
            )
            .with_owner(persistent_owner(OWNER)),
        },
    ]));
    assert!(add.applied, "{:?}", add.diagnostics);
    let accepted = session.document().clone();
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.undo_authoring().applied);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert!(session.redo_authoring().applied);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn authoring_preview_projects_only_exact_available_direct_records() {
    let (bundle, adapter) = scene_bundle_and_name_adapter();
    let owner = persistent_owner(OWNER);
    let entity = scene_id("player");
    let mut source_entity = SceneEntityRecord::new(entity.clone())
        .with_component(
            component_id("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Player".to_owned()),
            )
            .with_owner(owner.clone()),
        )
        .with_component(
            component_id("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("visible".to_owned()),
            )
            .with_owner(owner.clone()),
        );
    let nested_source = AssetRef::path("prefabs/nested.prefab.json").unwrap();
    source_entity.prefab = Some(PrefabInstance {
        source: nested_source.clone(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::ReplaceComponent {
            entity: entity.clone(),
            component: component_id("nara.scene.Name"),
            value: SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Patched name".to_owned()),
            )
            .with_owner(owner),
        }]),
    });
    let source_scene = SceneDocument::from_ordered_roots([source_entity.clone()]);
    let source_prefab = PrefabDocument::from_ordered_roots([source_entity]);

    let published_scene = SceneDocumentCandidate::decode_json_bytes(
        source_scene.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring(
        source("scenes/preview.scene.json"),
        &bundle,
        &[adapter.clone()],
    )
    .unwrap()
    .publish();
    let scene_before = published_scene.document().clone();
    let scene_manifest_before = published_scene.unavailable_manifest().clone();

    let published_scene_preview =
        nara_scene::__private::project_published_authoring_scene_preview(&published_scene);

    assert_eq!(published_scene.document(), &scene_before);
    assert_eq!(
        published_scene.unavailable_manifest(),
        &scene_manifest_before
    );
    assert_eq!(scene_manifest_before.record_count(), 1);
    assert_eq!(
        published_scene_preview.document().entities[0]
            .components
            .keys()
            .map(ComponentTypeId::as_str)
            .collect::<Vec<_>>(),
        vec!["nara.scene.Name"]
    );
    assert!(
        published_scene_preview.document().entities[0]
            .prefab
            .is_none()
    );
    assert_eq!(published_scene_preview.omissions().len(), 2);
    assert!(matches!(
        published_scene_preview.omissions()[0].location(),
        nara_scene::__private::AuthoringPreviewOmissionLocation::DirectEntityComponent {
            entity: omitted_entity,
            component,
        } if omitted_entity == &entity && component.as_str() == "nara.scene.Visibility"
    ));
    assert_eq!(
        published_scene_preview.omissions()[0].reason(),
        nara_scene::__private::AuthoringPreviewOmissionReason::RecordUnavailable(
            AuthoringRecordReadiness::KnownUnbound
        )
    );
    assert!(matches!(
        published_scene_preview.omissions()[1].location(),
        nara_scene::__private::AuthoringPreviewOmissionLocation::NestedPrefab {
            entity: omitted_entity,
        } if omitted_entity == &entity
    ));
    assert_eq!(
        published_scene_preview.omissions()[1].reason(),
        nara_scene::__private::AuthoringPreviewOmissionReason::NestedPrefabUnsupported
    );
    assert!(
        scene_before.entities[0]
            .prefab
            .as_ref()
            .is_some_and(|prefab| !prefab.overrides.is_empty())
    );

    let scene_session = SceneAuthoringSession::from_published_authoring_document(published_scene);
    let scene_session_preview =
        nara_scene::__private::project_authoring_scene_session_preview(&scene_session).unwrap();

    assert_eq!(scene_session.document(), &scene_before);
    assert_eq!(
        scene_session.unavailable_record_manifest(),
        Some(&scene_manifest_before)
    );
    assert_eq!(scene_session_preview, published_scene_preview);

    let published_prefab = PrefabDocumentCandidate::decode_json_bytes(
        source_prefab.to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring(source("prefabs/preview.prefab.json"), &bundle, &[adapter])
    .unwrap()
    .publish();
    let prefab_before = published_prefab.document().clone();
    let prefab_manifest_before = published_prefab.unavailable_manifest().clone();
    let published_prefab_preview =
        nara_scene::__private::project_published_authoring_prefab_preview(&published_prefab);

    assert_eq!(published_prefab.document(), &prefab_before);
    assert_eq!(
        published_prefab.unavailable_manifest(),
        &prefab_manifest_before
    );
    assert_eq!(
        published_prefab_preview.document().entities[0].components,
        published_scene_preview.document().entities[0].components
    );
    assert!(
        published_prefab_preview.document().entities[0]
            .prefab
            .is_none()
    );
    assert_eq!(
        published_prefab_preview.omissions(),
        published_scene_preview.omissions()
    );

    let prefab_session =
        PrefabAuthoringSession::from_published_authoring_document(published_prefab);
    let prefab_session_preview =
        nara_scene::__private::project_authoring_prefab_session_preview(&prefab_session).unwrap();

    assert_eq!(prefab_session.document(), &prefab_before);
    assert_eq!(
        prefab_session.unavailable_record_manifest(),
        Some(&prefab_manifest_before)
    );
    assert_eq!(prefab_session_preview, published_prefab_preview);
}

#[test]
fn single_authoring_field_edit_obeys_the_value_budget() {
    let error = AuthoringFieldEdit::single(
        scene_id("player"),
        component_id("nara.scene.Name"),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("value"),
        ComponentValue::String("x".repeat(65 * 1024)),
    )
    .unwrap_err();

    assert_eq!(
        error,
        nara_scene::AuthoringFieldEditError::ValueBudgetExceeded
    );
}

#[test]
fn cross_component_authoring_transaction_preserves_unknown_records_and_one_undo() {
    let (bundle, adapter, document, patch) = cross_component_authoring_fixture();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/transaction.scene.json"), &bundle, &[adapter])
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let original = session.document().clone();
    let revision = session.revision();
    let report = session.apply_authoring_patch(&patch);

    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(session.revision().generation(), revision.generation() + 1);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    let accepted = session.document().clone();
    let accepted_revision = session.revision();
    let accepted_state = session.content_state();
    let accepted_history = session.history_status();
    let mut invalid = patch.clone();
    invalid.operations[1] = ScenePatchOperation::SetField {
        entity: scene_id("player"),
        component: component_id("nara.future.Unknown"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::U64(99),
    };
    let rejected = session.apply_authoring_patch(&invalid);
    assert!(!rejected.applied);
    assert_eq!(session.document(), &accepted);
    assert_eq!(session.revision(), accepted_revision);
    assert_eq!(session.content_state(), accepted_state);
    assert_eq!(session.history_status(), accepted_history);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert_eq!(session.history_status().undo_depth, 0);
    assert_eq!(session.history_status().redo_depth, 1);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn authoring_transaction_bounds_forward_and_inverse_values_before_publication() {
    for large_source in [false, true] {
        let (bundle, adapter, mut document, mut patch) = cross_component_authoring_fixture();
        if large_source {
            for component in ["nara.scene.Name", "nara.scene.Visibility"] {
                document.entities[0]
                    .components
                    .get_mut(&component_id(component))
                    .unwrap()
                    .value = ComponentValue::String("x".repeat(40 * 1024));
            }
        } else {
            for operation in &mut patch.operations {
                let ScenePatchOperation::SetField { value, .. } = operation else {
                    unreachable!("the fixture contains only field writes");
                };
                *value = ComponentValue::String("x".repeat(40 * 1024));
            }
        }
        let published = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(source("scenes/bounded.scene.json"), &bundle, &[adapter])
        .unwrap()
        .publish();
        let mut session = SceneAuthoringSession::from_published_authoring_document(published);
        let original = session.document().clone();
        let revision = session.revision();
        let rejected = session.apply_authoring_patch(&patch);

        assert!(!rejected.applied);
        assert!(
            rejected.diagnostics.iter().any(|entry| {
                entry.code().as_str() == "scene.authoring-field-patch-unavailable"
            })
        );
        assert_eq!(session.document(), &original);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn cross_component_prefab_authoring_transaction_undo_restores_every_target() {
    let (bundle, adapter, document, patch) = cross_component_authoring_fixture();
    let document = PrefabDocument::new(document.roots, document.entities);
    let published =
        PrefabDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                source("prefabs/transaction.prefab.json"),
                &bundle,
                &[adapter],
            )
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let original = session.document().clone();
    let revision = session.revision();
    let report = session.apply_authoring_patch(&patch);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(session.revision().generation(), revision.generation() + 1);
    assert_eq!(session.history_status().undo_depth, 1);
    let accepted = session.document().clone();

    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
    assert_eq!(session.history_status().undo_depth, 0);
    assert_eq!(session.history_status().redo_depth, 1);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn available_field_edit_preserves_known_unbound_manifest_across_reopen() {
    let (bundle, adapter) = scene_bundle_and_name_adapter();
    let owner = persistent_owner(OWNER);
    let entity = scene_id("player");
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
        .with_component(
            component_id("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Player".to_owned()),
            )
            .with_owner(owner.clone()),
        )
        .with_component(
            component_id("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("visible".to_owned()),
            )
            .with_owner(owner),
        )]);
    let source = source("scenes/main.scene.json");
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source.clone(), &bundle, &[adapter.clone()])
            .unwrap()
            .publish();

    assert_eq!(published.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(
        published
            .record_facts()
            .iter()
            .map(|fact| (fact.component().as_str(), fact.readiness()))
            .collect::<Vec<_>>(),
        vec![
            ("nara.scene.Name", AuthoringRecordReadiness::Available),
            (
                "nara.scene.Visibility",
                AuthoringRecordReadiness::KnownUnbound,
            ),
        ]
    );
    assert_eq!(
        published.record_facts()[0]
            .adapter()
            .expect("Name is covered by the trusted Adapter")
            .identity()
            .as_str(),
        "nara.scene.first-party-authoring-adapter"
    );
    let manifest = published.unavailable_manifest().clone();
    assert_eq!(manifest.record_count(), 1);
    assert_eq!(manifest.patch_target_count(), 0);
    let diagnostics = published.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.code().as_str(), "scene.authoring-known-unbound");
    assert_eq!(
        diagnostic.severity(),
        nara_diagnostic::DiagnosticSeverity::Warning
    );

    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let before_revision = session.revision();
    let edit = AuthoringFieldEdit::single(
        entity,
        component_id("nara.scene.Name"),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("value"),
        ComponentValue::String("Renamed".to_owned()),
    )
    .unwrap();
    let report = session.apply_authoring_field_edit(edit);

    assert!(report.applied, "{:#?}", report.diagnostics);
    assert_ne!(session.revision(), before_revision);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[0]
            .components
            .get(&component_id("nara.scene.Name"))
            .unwrap()
            .value,
        ComponentValue::String("Renamed".to_owned())
    );

    let registry = ComponentRegistry::new();
    let undo = session.undo(&registry);
    assert!(undo.applied, "{:#?}", undo.diagnostics);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[0]
            .components
            .get(&component_id("nara.scene.Name"))
            .unwrap()
            .value,
        ComponentValue::String("Player".to_owned())
    );

    let redo = session.redo(&registry);
    assert!(redo.applied, "{:#?}", redo.diagnostics);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(
        session.document().entities[0]
            .components
            .get(&component_id("nara.scene.Name"))
            .unwrap()
            .value,
        ComponentValue::String("Renamed".to_owned())
    );

    let before_document = session.document().clone();
    let before_revision = session.revision();
    let before_history = session.history_status();
    let replacement = SceneDocument::default();
    assert_eq!(
        session.replace_document(replacement),
        Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument)
    );
    let native_candidate = SceneDocumentCandidate::decode_json_bytes(
        SceneDocument::default()
            .to_json_string()
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    assert!(matches!(
        session.try_replace_file_candidate(native_candidate, &registry),
        Err(AuthoringSessionReplacementError::Authority(
            AuthoringSessionAuthorityError::PublishedAuthoringDocument
        ))
    ));
    let mut world = World::new();
    let world_entities_before_sync = world.iter_entities().count();
    assert_eq!(
        session.sync_world(&mut world, &registry),
        Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument)
    );
    assert_eq!(world.iter_entities().count(), world_entities_before_sync);
    let raw_patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("player"),
        component: component_id("nara.scene.Name"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String("Bypass".to_owned()),
    }]);
    let raw_report = session.apply_patch(&raw_patch, &registry);
    assert!(!raw_report.applied);
    assert!(raw_report.diagnostics.has_errors());
    assert_eq!(session.document(), &before_document);
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.history_status(), before_history);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

    let reopened_candidate = SceneDocumentCandidate::decode_json_bytes(
        session.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap();
    let reopened = session
        .try_reopen_authoring_candidate(reopened_candidate)
        .unwrap();
    assert_eq!(reopened.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(reopened.document(), session.document());
}

#[test]
fn prefab_unknown_records_and_target_barriers_round_trip_semantically() {
    let unknown = component_id("nara.project.MissingBehavior");
    let anchor = scene_id("anchor");
    let added = scene_id("added");
    let overrides = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(added.clone()).with_component(
                unknown.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
                )
                .with_owner(persistent_owner("nara.project.behavior")),
            ),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        },
        ScenePatchOperation::SetField {
            entity: added,
            component: unknown.clone(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("enabled"),
            value: ComponentValue::Bool(false),
        },
    ]);
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord {
        id: anchor,
        children: Vec::new(),
        components: BTreeMap::from([(
            unknown,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("speed", ComponentValue::I64(3))]),
            )
            .with_owner(persistent_owner("nara.project.behavior")),
        )]),
        prefab: Some(PrefabInstance {
            source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
            overrides,
        }),
    }]);
    let source = source("prefabs/missing.prefab.json");
    let bundle =
        AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
    let encoded = prefab.to_json_string().unwrap();
    let published = PrefabDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .admit_authoring(source, &bundle, &[])
        .unwrap()
        .publish();

    assert_eq!(published.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(published.unavailable_manifest().record_count(), 2);
    assert_eq!(published.unavailable_manifest().patch_target_count(), 1);
    assert!(
        published
            .record_facts()
            .iter()
            .all(|fact| { fact.readiness() == AuthoringRecordReadiness::UnknownSchema })
    );

    let manifest = published.unavailable_manifest().clone();
    let document = published.document().clone();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let session_revision = session.revision();
    assert_eq!(
        session.replace_document(PrefabDocument::default()),
        Err(AuthoringSessionAuthorityError::PublishedAuthoringDocument)
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), session_revision);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

    let reopened = PrefabDocumentCandidate::decode_json_bytes(
        session.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap();
    let reopened = session.try_reopen_authoring_candidate(reopened).unwrap();
    assert_eq!(
        reopened.unavailable_record_manifest(),
        session.unavailable_record_manifest()
    );
}

#[test]
fn strict_runtime_rejects_degraded_scene_and_prefab_before_world_mutation() {
    let unknown = component_id("nara.project.MissingBehavior");
    let owner = persistent_owner("nara.project.behavior");
    let entity = SceneEntityRecord::new(scene_id("root")).with_component(
        unknown,
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("enabled", ComponentValue::Bool(true))]),
        )
        .with_owner(owner),
    );
    let scene = SceneDocument::from_ordered_roots([entity.clone()]);
    let prefab = PrefabDocument::from_ordered_roots([entity]);
    let bundle =
        AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
    let scene =
        SceneDocumentCandidate::decode_json_bytes(scene.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/degraded.scene.json"), &bundle, &[])
            .unwrap()
            .publish();
    let prefab =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("prefabs/degraded.prefab.json"), &bundle, &[])
            .unwrap()
            .publish();
    assert_eq!(scene.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(prefab.readiness(), AuthoringDocumentReadiness::Degraded);

    let mut registry = ComponentRegistry::new();
    let provider =
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    nara_reflect::__private::register_or_validate_resolved_schema_provider(&mut registry, provider)
        .unwrap();
    registry.freeze().unwrap();
    let mut world = World::new();
    let before = world.iter_entities().count();

    let scene_report = spawn_scene(&mut world, &registry, scene.document());
    assert!(scene_report.diagnostics.has_errors());
    assert!(scene_report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before);

    let prefab_report = spawn_prefab(&mut world, &registry, prefab.document());
    assert!(prefab_report.diagnostics.has_errors());
    assert!(prefab_report.instance.is_none());
    assert_eq!(world.iter_entities().count(), before);

    let mismatched_owner =
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("wrong-owner"))
            .with_component(
                component_id("nara.scene.Name"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("Wrong owner".to_owned()),
                )
                .with_owner(persistent_owner("nara.project.spoofed-scene-owner")),
            )]);
    let owner_report = spawn_scene(&mut world, &registry, &mismatched_owner);
    assert!(owner_report.instance.is_none());
    assert!(
        owner_report
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.component-owner-mismatch" })
    );
    assert_eq!(world.iter_entities().count(), before);
}

#[test]
fn older_and_future_records_classify_without_native_migration() {
    let component = component_id("nara.project.Versioned");
    let owner = persistent_owner("nara.project.versioned");
    let schema = ComponentSchema::new(component.clone(), "Versioned", ComponentSchemaVersion(2))
        .with_capabilities([ComponentCapability::Scene]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("old")).with_component(
            component.clone(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null)
                .with_owner(owner.clone()),
        ),
        SceneEntityRecord::new(scene_id("future")).with_component(
            component,
            SceneComponentRecord::new(ComponentSchemaVersion(3), ComponentValue::Null)
                .with_owner(owner),
        ),
    ]);
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/versioned.scene.json"), &bundle, &[])
            .unwrap()
            .publish();

    assert_eq!(
        published
            .record_facts()
            .iter()
            .map(|fact| fact.readiness())
            .collect::<Vec<_>>(),
        vec![
            AuthoringRecordReadiness::FutureSchema,
            AuthoringRecordReadiness::Unmigratable,
        ]
    );
}

#[test]
fn unavailable_manifest_retains_entity_and_schema_declared_asset_references() {
    let component = component_id("nara.project.Link");
    let owner = persistent_owner("nara.project.link");
    let target = scene_id("target");
    let asset = AssetRef::path("textures/icon.png").unwrap();
    let schema = ComponentSchema::new(component.clone(), "Link", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueKind::EntityRef,
            )
            .with_capability(ComponentCapability::EntityRef),
            ComponentFieldSchema::required(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
            )
            .with_capability(ComponentCapability::AssetRef),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..ComponentSchemaCatalog::default()
        },
        None,
    )])
    .unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("source")).with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: target.clone(),
                        }),
                    ),
                    (
                        "image",
                        ComponentValue::map([
                            ("kind", ComponentValue::String("path".to_owned())),
                            (
                                "value",
                                ComponentValue::String("textures/icon.png".to_owned()),
                            ),
                        ]),
                    ),
                ]),
            )
            .with_owner(owner),
        ),
        SceneEntityRecord::new(target),
    ]);
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/links.scene.json"), &bundle, &[])
            .unwrap()
            .publish();

    let UnavailableRecordManifestEntry::Record(record) =
        &published.unavailable_manifest().entries()[0]
    else {
        panic!("the link record must be retained as an unavailable record")
    };
    assert_eq!(record.entity_references().len(), 1);
    assert_eq!(
        record.entity_references()[0].path(),
        &[UnavailableValuePathSegment::Field("target".to_owned())]
    );
    assert_eq!(
        record.entity_references()[0].reference(),
        &EntityReference::SceneLocal {
            entity: scene_id("target")
        }
    );
    assert_eq!(record.schema_declared_asset_references().len(), 1);
    assert_eq!(
        record.schema_declared_asset_references()[0].asset_ref(),
        &asset
    );
}

#[test]
fn unknown_component_value_tag_rejects_before_authoring_candidate() {
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("entity"))
        .with_component(
            component_id("nara.project.UnknownValue"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )]);
    let mut encoded =
        serde_json::from_str::<serde_json::Value>(&document.to_json_string().unwrap()).unwrap();
    encoded["payload"]["entities"][0]["components"]["nara.project.UnknownValue"]["value"] =
        serde_json::json!({ "type": "future_value", "value": 7 });

    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes(&serde_json::to_vec(&encoded).unwrap()),
        Err(SceneFormatError::Payload { .. })
    ));
}

fn cross_component_authoring_fixture() -> (
    AuthoringSchemaBundle,
    nara_scene::__private::AuthoringAdapterReceipt,
    SceneDocument,
    ScenePatchDocument,
) {
    let (bundle, _) = scene_bundle_and_name_adapter();
    let resolved =
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    let name = component_id("nara.scene.Name");
    let visibility = component_id("nara.scene.Visibility");
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_trusted_provider(
        resolved.contribution_receipt(),
        "nara.scene.first-party-authoring-adapter",
        1,
        1,
        [name.clone(), visibility.clone()],
    )
    .unwrap();
    let entity = scene_id("player");
    let owner = persistent_owner(OWNER);
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
        .with_component(
            name.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Player".to_owned()),
            )
            .with_owner(owner.clone()),
        )
        .with_component(
            visibility.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("visible".to_owned()),
            )
            .with_owner(owner),
        )
        .with_component(
            component_id("nara.future.Unknown"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7))
                .with_owner(persistent_owner("nara.future")),
        )]);
    let patch = ScenePatchDocument::new([(name, "Hero"), (visibility, "hidden")].into_iter().map(
        |(component, value)| ScenePatchOperation::SetField {
            entity: entity.clone(),
            component,
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("value"),
            value: ComponentValue::String(value.to_owned()),
        },
    ));
    (bundle, adapter, document, patch)
}

fn scene_bundle_and_name_adapter() -> (
    AuthoringSchemaBundle,
    nara_scene::__private::AuthoringAdapterReceipt,
) {
    let resolved =
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        persistent_owner(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str()),
        resolved.current().catalog().clone(),
        resolved
            .predecessor()
            .map(|record| record.catalog().clone()),
    )])
    .unwrap();
    let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_trusted_provider(
        resolved.contribution_receipt(),
        "nara.scene.first-party-authoring-adapter",
        1,
        1,
        [component_id("nara.scene.Name")],
    )
    .unwrap();
    (bundle, adapter)
}

fn source(path: &str) -> AuthoringDocumentSource {
    AuthoringDocumentSource::new(AssetRef::path(path).unwrap())
}

fn persistent_owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn component_id(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
