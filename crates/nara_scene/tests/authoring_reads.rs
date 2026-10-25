#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringEntityView,
    AuthoringPropertyValue, AuthoringReadError, AuthoringRecordReadiness, PrefabAuthoringSession,
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
    let AuthoringPropertyValue::Present(amount) = data
        .property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("amount")),
            &mut read_budget(),
        )
        .unwrap()
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
        data.property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("defaulted")),
            &mut read_budget()
        )
        .unwrap(),
        AuthoringPropertyValue::Defaulted(&ComponentValue::U64(19))
    );
    assert_eq!(
        data.property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("missing")),
            &mut read_budget()
        )
        .unwrap(),
        AuthoringPropertyValue::Missing
    );
    assert_eq!(
        data.property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("nullable")),
            &mut read_budget()
        )
        .unwrap(),
        AuthoringPropertyValue::Present(&ComponentValue::Null)
    );
    assert_eq!(
        data.property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("null_default")),
            &mut read_budget()
        )
        .unwrap(),
        AuthoringPropertyValue::Defaulted(&ComponentValue::Null)
    );
    assert_eq!(
        data.property_value(
            &ComponentPropertyPath::new(ComponentFieldId::new("unknown")),
            &mut read_budget()
        )
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
                .property_value(
                    &ComponentPropertyPath::new(ComponentFieldId::new("amount")),
                    &mut read_budget()
                )
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
                    .property_value(
                        &ComponentPropertyPath::new(read("amount")),
                        &mut read_budget()
                    )
                    .unwrap(),
                AuthoringPropertyValue::Present(&ComponentValue::U64(7))
            );
            assert!($session.commit_authoring_edit(prepared).applied);
            let view = $session
                .authoring_entity_view(&id("populated"))
                .unwrap()
                .component(&component)
                .unwrap();
            assert_eq!(
                view.property_value(
                    &ComponentPropertyPath::new(read("amount")),
                    &mut read_budget()
                )
                .unwrap(),
                AuthoringPropertyValue::Present(&ComponentValue::U64(31))
            );
            assert_eq!(
                view.property_value(
                    &ComponentPropertyPath::new(read("defaulted")),
                    &mut read_budget()
                )
                .unwrap(),
                AuthoringPropertyValue::Present(&ComponentValue::U64(24))
            );
            assert!($session.undo_authoring().applied);
            let view = $session
                .authoring_entity_view(&id("populated"))
                .unwrap()
                .component(&component)
                .unwrap();
            assert_eq!(
                view.property_value(
                    &ComponentPropertyPath::new(read("amount")),
                    &mut read_budget()
                )
                .unwrap(),
                AuthoringPropertyValue::Present(&ComponentValue::U64(7))
            );
            assert_eq!(
                view.property_value(
                    &ComponentPropertyPath::new(read("defaulted")),
                    &mut read_budget()
                )
                .unwrap(),
                AuthoringPropertyValue::Defaulted(&ComponentValue::U64(19))
            );
            assert!($session.redo_authoring().applied);
            assert!($session.apply_authoring_patch(&reset).applied);
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .property_value(
                        &ComponentPropertyPath::new(read("defaulted")),
                        &mut read_budget()
                    )
                    .unwrap(),
                AuthoringPropertyValue::Defaulted(&ComponentValue::U64(19))
            );
            assert!($session.undo_authoring().applied);
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .property_value(
                        &ComponentPropertyPath::new(read("defaulted")),
                        &mut read_budget()
                    )
                    .unwrap(),
                AuthoringPropertyValue::Present(&ComponentValue::U64(24))
            );
            assert!($session.redo_authoring().applied);
            assert_eq!(
                $session
                    .authoring_entity_view(&id("populated"))
                    .unwrap()
                    .component(&component)
                    .unwrap()
                    .property_value(
                        &ComponentPropertyPath::new(read("defaulted")),
                        &mut read_budget()
                    )
                    .unwrap(),
                AuthoringPropertyValue::Defaulted(&ComponentValue::U64(19))
            );
        }};
    }
    check!(scene);
    check!(prefab);
}

fn read_budget() -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(
        nara_core::ItemLimit::new(4096).unwrap(),
        nara_core::ByteLimit::new(65536).unwrap(),
    )
}

#[test]
fn selected_descendants_keep_default_origin_and_distinguish_unavailable_targets() {
    use nara_reflect::{
        ComponentPropertyError, ComponentPropertyStep, ComponentShapeMember, ComponentValueShape,
    };
    let component = ComponentTypeId::new("game.Nested");
    let shape = ComponentValueShape::record_members([ComponentShapeMember::new(
        "wire_score",
        ComponentValueKind::U64,
    )
    .with_id(ComponentFieldId::new("score"))]);
    let field = |name: &str| ComponentFieldId::new(name);
    let schema = ComponentSchema::new(component.clone(), "Nested", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::optional_with_default(
                field("root"),
                "Root",
                ComponentFieldPath::from_fields(["root"]),
                shape.clone(),
                ComponentValue::map([("wire_score", ComponentValue::U64(11))]),
            ),
            ComponentFieldSchema::optional(
                field("absent"),
                "Absent",
                ComponentFieldPath::from_fields(["absent"]),
                shape.clone(),
            ),
            ComponentFieldSchema::required(
                field("maybe"),
                "Maybe",
                ComponentFieldPath::from_fields(["maybe"]),
                ComponentValueShape::optional(shape),
            ),
        ])
        .with_field_tombstones([
            field("zeta"),
            field("retired"),
            field("alpha"),
            field("middle"),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.nested").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapters =
        [
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap(),
        ];
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("defaults")).with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("maybe", ComponentValue::Null)]),
            ),
        ),
        SceneEntityRecord::new(id("explicit")).with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("maybe", ComponentValue::Null),
                    (
                        "root",
                        ComponentValue::map([("wire_score", ComponentValue::U64(7))]),
                    ),
                ]),
            ),
        ),
    ]);
    let source = AuthoringDocumentSource::new(AssetRef::path("scenes/nested.scene.json").unwrap());
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
    let selected = |root: &str| {
        ComponentPropertyPath::new(field(root))
            .with_steps([ComponentPropertyStep::Member(field("score"))])
    };
    macro_rules! check {
        ($session:expr) => {{
            for (entity, expected) in [
                (
                    "defaults",
                    AuthoringPropertyValue::Defaulted(&ComponentValue::U64(11)),
                ),
                (
                    "explicit",
                    AuthoringPropertyValue::Present(&ComponentValue::U64(7)),
                ),
            ] {
                let view = $session
                    .authoring_entity_view(&id(entity))
                    .unwrap()
                    .component(&component)
                    .unwrap();
                assert_eq!(
                    view.property_value(&selected("root"), &mut read_budget())
                        .unwrap(),
                    expected
                );
                assert!(matches!(
                    view.property_value(&selected("maybe"), &mut read_budget()),
                    Err(AuthoringReadError::Property(
                        ComponentPropertyError::OptionalAbsent { step: 0 }
                    ))
                ));
                assert_eq!(
                    view.property_value(
                        &ComponentPropertyPath::new(field("absent")),
                        &mut read_budget()
                    )
                    .unwrap(),
                    AuthoringPropertyValue::Missing
                );
                assert!(matches!(
                    view.property_value(&selected("absent"), &mut read_budget()),
                    Err(AuthoringReadError::Property(
                        ComponentPropertyError::ValueUnavailable(_)
                    ))
                ));
                for retired in ["alpha", "middle", "retired", "zeta"] {
                    assert_eq!(
                        view.property_value(
                            &ComponentPropertyPath::new(field(retired)),
                            &mut read_budget()
                        ),
                        Err(AuthoringReadError::Property(
                            ComponentPropertyError::RetiredField(field(retired))
                        ))
                    );
                }
                assert_eq!(
                    view.property_value(
                        &ComponentPropertyPath::new(field("unknown")),
                        &mut read_budget()
                    ),
                    Err(AuthoringReadError::FieldNotFound)
                );
                let mut minimum = ComponentPropertyReadBudget::new(
                    nara_core::ItemLimit::ONE,
                    nara_core::ByteLimit::ONE,
                );
                assert!(
                    view.property_value(&ComponentPropertyPath::new(field("root")), &mut minimum)
                        .is_ok()
                );
                assert_eq!(minimum.scanned_entries(), 0);
            }
        }};
    }
    check!(scene);
    check!(prefab);
}
