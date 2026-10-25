#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringEntityView,
    AuthoringReadError, AuthoringRecordReadiness, AuthoringRootValue, PrefabAuthoringSession,
    PrefabDocument, PrefabDocumentCandidate, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn bundle() -> AuthoringSchemaBundle {
    let schema = |name, version| {
        ComponentSchema::new(ComponentTypeId::new(name), name, version)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    };
    let field = |name: &str| ComponentFieldId::new(name);
    let path = |name| ComponentFieldPath::from_fields(["wire", name]);
    let data = schema("game.Data", ComponentSchemaVersion::ONE).with_fields([
        ComponentFieldSchema::required(
            field("amount"),
            "Amount",
            path("amount"),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional_with_default(
            field("defaulted"),
            "Defaulted",
            path("defaulted"),
            ComponentValueKind::U64,
            ComponentValue::U64(19),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::optional(
            field("missing"),
            "Missing",
            path("missing"),
            ComponentValueKind::String,
        ),
        ComponentFieldSchema::optional_with_default(
            field("nullable"),
            "Nullable",
            path("nullable"),
            ComponentValueKind::String,
            ComponentValue::String("fallback".into()),
        ),
        ComponentFieldSchema::optional_with_default(
            field("null_default"),
            "Null default",
            path("null_default"),
            ComponentValueKind::String,
            ComponentValue::Null,
        ),
        ComponentFieldSchema::required(
            field("blob"),
            "Blob",
            ComponentFieldPath::from_fields(["blob"]),
            ComponentValueKind::String,
        ),
    ]);
    AuthoringSchemaBundle::new([
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.available").unwrap(),
            ComponentSchemaCatalog {
                components: vec![
                    data,
                    schema("game.Future", ComponentSchemaVersion::ONE),
                    schema("game.Old", ComponentSchemaVersion::new(2).unwrap()),
                    schema("game.Creatable", ComponentSchemaVersion::ONE),
                ],
                ..Default::default()
            },
            None,
        ),
        AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.unbound").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema("game.Unbound", ComponentSchemaVersion::ONE)],
                ..Default::default()
            },
            None,
        ),
    ])
    .unwrap()
}

fn document() -> SceneDocument {
    let empty = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let record = |version, value| SceneComponentRecord::new(version, value);
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("empty")),
        SceneEntityRecord::new(id("populated"))
            .with_component(
                ComponentTypeId::new("game.Data"),
                record(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        (
                            "wire",
                            ComponentValue::map([
                                ("amount", ComponentValue::U64(7)),
                                ("nullable", ComponentValue::Null),
                            ]),
                        ),
                        ("blob", ComponentValue::String("x".repeat(48_000))),
                    ]),
                ),
            )
            .with_component(
                ComponentTypeId::new("game.Future"),
                record(ComponentSchemaVersion::new(2).unwrap(), empty.clone()),
            )
            .with_component(
                ComponentTypeId::new("game.Old"),
                record(ComponentSchemaVersion::ONE, empty.clone()),
            )
            .with_component(
                ComponentTypeId::new("game.Unbound"),
                record(ComponentSchemaVersion::ONE, empty.clone()),
            )
            .with_component(
                ComponentTypeId::new("game.Unknown"),
                record(ComponentSchemaVersion::ONE, empty),
            ),
        SceneEntityRecord::new(id("unavailable-only")).with_component(
            ComponentTypeId::new("game.Unknown"),
            record(ComponentSchemaVersion::ONE, ComponentValue::Null),
        ),
    ])
}

fn sessions() -> (SceneAuthoringSession, PrefabAuthoringSession) {
    let bundle = bundle();
    let adapters =
        [
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ];
    let source = AuthoringDocumentSource::new(AssetRef::path("scenes/read.scene.json").unwrap());
    let document = document();
    let scene = SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source.clone(), &bundle, &adapters)
            .unwrap()
            .publish(),
    );
    let prefab = PrefabDocument::from_ordered_roots(document.entities);
    let prefab = PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source, &bundle, &adapters)
            .unwrap()
            .publish(),
    );
    (scene, prefab)
}

fn assert_actual_inventory(view: AuthoringEntityView<'_>, original: &SceneEntityRecord) {
    let headers = view
        .components_after(None)
        .map(|component| {
            (
                component.fact().component().as_str().to_owned(),
                component.fact().readiness(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        headers,
        [
            ("game.Data".into(), AuthoringRecordReadiness::Available),
            ("game.Future".into(), AuthoringRecordReadiness::FutureSchema),
            ("game.Old".into(), AuthoringRecordReadiness::Unmigratable),
            (
                "game.Unbound".into(),
                AuthoringRecordReadiness::KnownUnbound
            ),
            (
                "game.Unknown".into(),
                AuthoringRecordReadiness::UnknownSchema
            ),
        ]
    );
    assert_eq!(
        view.components_after(Some(&ComponentTypeId::new("game.Old")))
            .count(),
        2
    );
    assert_eq!(
        view.component(&ComponentTypeId::new("game.Creatable"))
            .unwrap_err(),
        AuthoringReadError::ComponentNotFound
    );
    let data = view.component(&ComponentTypeId::new("game.Data")).unwrap();
    assert_eq!(data.root_fields().unwrap().len(), 6);
    let AuthoringRootValue::Present(amount) =
        data.root_value(&ComponentFieldId::new("amount")).unwrap()
    else {
        panic!("expected explicit amount")
    };
    assert_eq!(amount, &ComponentValue::U64(7));
    assert!(std::ptr::eq(
        amount,
        original.components[&ComponentTypeId::new("game.Data")]
            .value
            .get("wire")
            .unwrap()
            .get("amount")
            .unwrap()
    ));
    assert_eq!(
        data.root_value(&ComponentFieldId::new("defaulted"))
            .unwrap(),
        AuthoringRootValue::Defaulted(&ComponentValue::U64(19))
    );
    assert_eq!(
        data.root_value(&ComponentFieldId::new("missing")).unwrap(),
        AuthoringRootValue::Missing
    );
    assert_eq!(
        data.root_value(&ComponentFieldId::new("nullable")).unwrap(),
        AuthoringRootValue::Present(&ComponentValue::Null)
    );
    assert_eq!(
        data.root_value(&ComponentFieldId::new("null_default"))
            .unwrap(),
        AuthoringRootValue::Defaulted(&ComponentValue::Null)
    );
    assert_eq!(
        data.root_value(&ComponentFieldId::new("unknown"))
            .unwrap_err(),
        AuthoringReadError::FieldNotFound
    );
    for (name, readiness) in headers.into_iter().skip(1) {
        let component = view.component(&ComponentTypeId::new(name)).unwrap();
        assert_eq!(
            component.root_fields().unwrap_err(),
            AuthoringReadError::UnavailableComponent { readiness }
        );
        assert_eq!(
            component
                .root_value(&ComponentFieldId::new("amount"))
                .unwrap_err(),
            AuthoringReadError::UnavailableComponent { readiness }
        );
    }
}

#[test]
fn borrowed_scene_and_prefab_reads_distinguish_actual_readiness_defaults_missing_and_null() {
    let (scene, prefab) = sessions();
    assert_actual_inventory(
        scene.authoring_entity_view(&id("populated")).unwrap(),
        &scene.document().entities[1],
    );
    assert_actual_inventory(
        prefab.authoring_entity_view(&id("populated")).unwrap(),
        &prefab.document().entities[1],
    );
    assert_eq!(
        scene
            .authoring_entity_view(&id("empty"))
            .unwrap()
            .components_after(None)
            .count(),
        0
    );
    assert_eq!(
        prefab
            .authoring_entity_view(&id("empty"))
            .unwrap()
            .components_after(None)
            .count(),
        0
    );
    assert_eq!(
        scene
            .authoring_entity_view(&id("unavailable-only"))
            .unwrap()
            .components_after(None)
            .count(),
        1
    );
    assert_eq!(
        prefab
            .authoring_entity_view(&id("unavailable-only"))
            .unwrap()
            .components_after(None)
            .count(),
        1
    );
    assert_eq!(
        scene.authoring_entity_view(&id("absent")).unwrap_err(),
        AuthoringReadError::EntityNotFound
    );
    assert_eq!(
        prefab.authoring_entity_view(&id("absent")).unwrap_err(),
        AuthoringReadError::EntityNotFound
    );
    let native = SceneAuthoringSession::new(SceneDocument::default());
    assert_eq!(
        native.authoring_entity_view(&id("absent")).unwrap_err(),
        AuthoringReadError::NotAdmitted
    );
}

#[test]
fn read_indexes_follow_only_published_commit_undo_redo_and_reopen() {
    let (mut scene, mut prefab) = sessions();
    let invalid = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id("populated")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    assert!(
        scene
            .prepare_authoring_edit(scene.revision(), &invalid)
            .is_err()
    );
    assert!(
        prefab
            .prepare_authoring_edit(prefab.revision(), &invalid)
            .is_err()
    );
    assert_actual_inventory(
        scene.authoring_entity_view(&id("populated")).unwrap(),
        &scene.document().entities[1],
    );
    assert_actual_inventory(
        prefab.authoring_entity_view(&id("populated")).unwrap(),
        &prefab.document().entities[1],
    );
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id("added")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let scene_prepared = scene
        .prepare_authoring_edit(scene.revision(), &patch)
        .unwrap();
    let prefab_prepared = prefab
        .prepare_authoring_edit(prefab.revision(), &patch)
        .unwrap();
    assert_eq!(
        scene.authoring_entity_view(&id("added")).unwrap_err(),
        AuthoringReadError::EntityNotFound
    );
    assert_eq!(
        prefab.authoring_entity_view(&id("added")).unwrap_err(),
        AuthoringReadError::EntityNotFound
    );
    assert!(scene.commit_authoring_edit(scene_prepared).applied);
    assert!(prefab.commit_authoring_edit(prefab_prepared).applied);
    assert!(scene.authoring_entity_view(&id("added")).is_ok());
    assert!(prefab.authoring_entity_view(&id("added")).is_ok());
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(
        scene.authoring_entity_view(&id("added")).unwrap_err(),
        AuthoringReadError::EntityNotFound
    );
    assert_eq!(
        prefab.authoring_entity_view(&id("added")).unwrap_err(),
        AuthoringReadError::EntityNotFound
    );
    assert!(scene.redo_authoring().applied);
    assert!(prefab.redo_authoring().applied);
    assert_actual_inventory(
        scene.authoring_entity_view(&id("populated")).unwrap(),
        scene
            .document()
            .entities
            .iter()
            .find(|record| record.id == id("populated"))
            .unwrap(),
    );
    assert_actual_inventory(
        prefab.authoring_entity_view(&id("populated")).unwrap(),
        prefab
            .document()
            .entities
            .iter()
            .find(|record| record.id == id("populated"))
            .unwrap(),
    );
    let reopened = scene
        .authoring_readmission()
        .unwrap()
        .reopen_scene_candidate(
            SceneDocumentCandidate::decode_json_bytes(
                scene.document().to_json_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_ne!(
        reopened.revision().source_id(),
        scene.revision().source_id()
    );
    assert!(reopened.authoring_entity_view(&id("added")).is_ok());
    assert_actual_inventory(
        reopened.authoring_entity_view(&id("populated")).unwrap(),
        reopened
            .document()
            .entities
            .iter()
            .find(|record| record.id == id("populated"))
            .unwrap(),
    );
    let reopened_prefab = prefab
        .authoring_readmission()
        .unwrap()
        .reopen_prefab_candidate(
            PrefabDocumentCandidate::decode_json_bytes(
                prefab.document().to_json_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_ne!(
        reopened_prefab.revision().source_id(),
        prefab.revision().source_id()
    );
    assert!(reopened_prefab.authoring_entity_view(&id("added")).is_ok());
    assert_actual_inventory(
        reopened_prefab
            .authoring_entity_view(&id("populated"))
            .unwrap(),
        reopened_prefab
            .document()
            .entities
            .iter()
            .find(|record| record.id == id("populated"))
            .unwrap(),
    );
}

#[test]
fn field_reads_follow_published_values_and_default_origin_through_history() {
    let (mut scene, mut prefab) = sessions();
    let component = ComponentTypeId::new("game.Data");
    let set = |field: &str, value| ScenePatchOperation::SetField {
        entity: id("populated"),
        component: component.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
        value: ComponentValue::U64(value),
    };
    let patch = ScenePatchDocument::new([set("amount", 31), set("defaulted", 24)]);
    let reset = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: id("populated"),
        component: component.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("defaulted"),
    }]);
    macro_rules! check {
        ($session:ident) => {{
            let read = |field: &str| ComponentFieldId::new(field);
            let prepared = $session
                .prepare_authoring_edit($session.revision(), &patch)
                .unwrap();
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .root_value(&read("amount"))
                    .unwrap(),
                AuthoringRootValue::Present(&ComponentValue::U64(7))
            );
            assert!($session.commit_authoring_edit(prepared).applied);
            let view = $session
                .authoring_entity_view(&id("populated"))
                .unwrap()
                .component(&component)
                .unwrap();
            assert_eq!(
                view.root_value(&read("amount")).unwrap(),
                AuthoringRootValue::Present(&ComponentValue::U64(31))
            );
            assert_eq!(
                view.root_value(&read("defaulted")).unwrap(),
                AuthoringRootValue::Present(&ComponentValue::U64(24))
            );
            assert!($session.undo_authoring().applied);
            let view = $session
                .authoring_entity_view(&id("populated"))
                .unwrap()
                .component(&component)
                .unwrap();
            assert_eq!(
                view.root_value(&read("amount")).unwrap(),
                AuthoringRootValue::Present(&ComponentValue::U64(7))
            );
            assert_eq!(
                view.root_value(&read("defaulted")).unwrap(),
                AuthoringRootValue::Defaulted(&ComponentValue::U64(19))
            );
            assert!($session.redo_authoring().applied);
            assert!($session.apply_authoring_patch(&reset).applied);
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .root_value(&read("defaulted"))
                    .unwrap(),
                AuthoringRootValue::Defaulted(&ComponentValue::U64(19))
            );
            assert!($session.undo_authoring().applied);
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .root_value(&read("defaulted"))
                    .unwrap(),
                AuthoringRootValue::Present(&ComponentValue::U64(24))
            );
            assert!($session.redo_authoring().applied);
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .root_value(&read("defaulted"))
                    .unwrap(),
                AuthoringRootValue::Defaulted(&ComponentValue::U64(19))
            );
        }};
    }
    check!(scene);
    check!(prefab);
}
