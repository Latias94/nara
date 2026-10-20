#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringFieldEdit,
    AuthoringRecordLocation, AuthoringRecordReadiness, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, PrefabInstance, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation,
};

const OWNER: &str = "test.lookup.bound";
const COMPONENT: &str = "test.lookup.Value";
const CURRENT: ComponentSchemaVersion = ComponentSchemaVersion(2);

#[test]
fn authoring_schema_lookup_requires_an_exact_available_direct_record_in_both_kinds() {
    let (bundle, adapters) = catalog();
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/nested.prefab.json").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
            entity: id("patch-only"),
            component: component(),
            value: record(CURRENT, OWNER),
        }]),
    });
    let document = SceneDocument::from_ordered_roots([
        entity("available", CURRENT, OWNER),
        entity("old", ComponentSchemaVersion::ONE, OWNER),
        entity("future", ComponentSchemaVersion(3), OWNER),
        entity("wrong-owner", CURRENT, "test.lookup.unbound"),
        SceneEntityRecord::new(id("unbound")).with_component(
            ComponentTypeId::new("test.lookup.Unbound"),
            record(CURRENT, "test.lookup.unbound"),
        ),
        SceneEntityRecord::new(id("unknown")).with_component(
            ComponentTypeId::new("test.lookup.Unknown"),
            record(CURRENT, OWNER),
        ),
        SceneEntityRecord::new(id("empty")),
        anchor,
    ]);
    let (scene, prefab) = sessions(&document, &bundle, &adapters);
    assert_eq!(
        scene
            .authoring_component_schema(&id("available"), &component())
            .unwrap()
            .version(),
        CURRENT
    );
    assert_eq!(
        prefab
            .authoring_component_schema(&id("available"), &component())
            .unwrap()
            .version(),
        CURRENT
    );
    for (entity, component) in [
        ("old", COMPONENT),
        ("future", COMPONENT),
        ("wrong-owner", COMPONENT),
        ("unbound", "test.lookup.Unbound"),
        ("unknown", "test.lookup.Unknown"),
        ("empty", COMPONENT),
        ("missing", COMPONENT),
        ("patch-only", COMPONENT),
        ("available", "test.lookup.Unknown"),
    ] {
        assert!(
            scene
                .authoring_component_schema(&id(entity), &ComponentTypeId::new(component))
                .is_none(),
            "{entity}/{component}"
        );
        assert!(
            prefab
                .authoring_component_schema(&id(entity), &ComponentTypeId::new(component))
                .is_none(),
            "{entity}/{component}"
        );
    }
    for (entity, expected) in [
        ("old", AuthoringRecordReadiness::Unmigratable),
        ("future", AuthoringRecordReadiness::FutureSchema),
        ("wrong-owner", AuthoringRecordReadiness::UnknownSchema),
        ("unbound", AuthoringRecordReadiness::KnownUnbound),
        ("unknown", AuthoringRecordReadiness::UnknownSchema),
    ] {
        let fact = scene.authoring_record_facts().unwrap().iter().find(|fact| {
            matches!(fact.location(), AuthoringRecordLocation::EntityComponent { entity: owner } if owner == &id(entity))
        }).unwrap();
        assert_eq!(fact.readiness(), expected);
    }
    assert!(scene.authoring_record_facts().unwrap().iter().any(|fact| {
        fact.readiness() == AuthoringRecordReadiness::Available
            && matches!(fact.location(), AuthoringRecordLocation::PatchComponent { entity, .. } if entity == &id("patch-only"))
    }), "an Available patch record must not become direct schema authority");
    let native = SceneAuthoringSession::new(document.clone());
    let native_prefab =
        PrefabAuthoringSession::new(PrefabDocument::new(document.roots, document.entities));
    assert!(
        native
            .authoring_component_schema(&id("available"), &component())
            .is_none()
    );
    assert!(
        native_prefab
            .authoring_component_schema(&id("available"), &component())
            .is_none()
    );
}

#[test]
fn authoring_schema_lookup_tracks_component_edits_and_exact_history_in_both_kinds() {
    let (bundle, adapters) = catalog();
    let document = SceneDocument::from_ordered_roots([entity("root", CURRENT, OWNER)]);
    let (mut scene, mut prefab) = sessions(&document, &bundle, &adapters);
    let removed = ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
        entity: id("root"),
        component: component(),
    }]);
    assert!(scene.apply_authoring_patch(&removed).applied);
    assert!(prefab.apply_authoring_patch(&removed).applied);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(scene.document(), &document);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(scene.redo_authoring().applied);
    assert!(prefab.redo_authoring().applied);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    let edit = AuthoringFieldEdit::single(
        id("root"),
        component(),
        CURRENT,
        ComponentFieldId::new("value"),
        ComponentValue::U64(7),
    )
    .unwrap();
    assert!(scene.apply_authoring_field_edit(edit.clone()).applied);
    assert!(prefab.apply_authoring_field_edit(edit).applied);
    assert_eq!(
        scene.document().entities[0].components[&component()]
            .value
            .field("value")
            .unwrap(),
        &ComponentValue::U64(7)
    );
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(scene.undo_authoring().applied);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(scene.document(), &document);
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
}

#[test]
fn authoring_schema_lookup_is_rebuilt_for_reopened_classification_without_mutating_origin() {
    let (bundle, adapters) = catalog();
    let document = SceneDocument::from_ordered_roots([entity("root", CURRENT, OWNER)]);
    let (scene, prefab) = sessions(&document, &bundle, &adapters);
    let changed =
        SceneDocument::from_ordered_roots([entity("root", ComponentSchemaVersion(3), OWNER)]);
    let reopened = scene
        .authoring_readmission()
        .unwrap()
        .reopen_scene_candidate(
            SceneDocumentCandidate::decode_json_bytes(changed.to_json_string().unwrap().as_bytes())
                .unwrap(),
        )
        .unwrap();
    let reopened_prefab = prefab
        .authoring_readmission()
        .unwrap()
        .reopen_prefab_candidate(
            PrefabDocumentCandidate::decode_json_bytes(
                PrefabDocument::new(changed.roots, changed.entities)
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        reopened
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert!(
        reopened_prefab
            .authoring_component_schema(&id("root"), &component())
            .is_none()
    );
    assert_eq!(
        reopened.authoring_record_facts().unwrap()[0].readiness(),
        AuthoringRecordReadiness::FutureSchema
    );
    assert!(
        scene
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert_eq!(scene.document(), &document);
    let restored = reopened
        .authoring_readmission()
        .unwrap()
        .reopen_scene_candidate(
            SceneDocumentCandidate::decode_json_bytes(
                document.to_json_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    let restored_prefab = reopened_prefab
        .authoring_readmission()
        .unwrap()
        .reopen_prefab_candidate(
            PrefabDocumentCandidate::decode_json_bytes(
                PrefabDocument::new(document.roots.clone(), document.entities.clone())
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        restored
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
    assert!(
        restored_prefab
            .authoring_component_schema(&id("root"), &component())
            .is_some()
    );
}

fn catalog() -> (AuthoringSchemaBundle, Vec<AuthoringAdapterReceipt>) {
    let bundle = AuthoringSchemaBundle::new([
        AuthoringSchemaBundleOwner::new(
            owner(OWNER),
            ComponentSchemaCatalog {
                components: vec![schema(COMPONENT)],
                ..ComponentSchemaCatalog::default()
            },
            None,
        ),
        AuthoringSchemaBundleOwner::new(
            owner("test.lookup.unbound"),
            ComponentSchemaCatalog {
                components: vec![schema("test.lookup.Unbound")],
                ..ComponentSchemaCatalog::default()
            },
            None,
        ),
    ])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    (bundle, vec![adapter])
}

fn schema(component: &str) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(component), "Lookup value", CURRENT)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
        .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(1))]))
}

fn sessions(
    document: &SceneDocument,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
) -> (SceneAuthoringSession, PrefabAuthoringSession) {
    let scene =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("scenes/lookup.scene.json"), bundle, adapters)
            .unwrap()
            .publish();
    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let prefab =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source("prefabs/lookup.prefab.json"), bundle, adapters)
            .unwrap()
            .publish();
    (
        SceneAuthoringSession::from_published_authoring_document(scene),
        PrefabAuthoringSession::from_published_authoring_document(prefab),
    )
}

fn entity(entity: &str, version: ComponentSchemaVersion, owner: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(entity)).with_component(component(), record(version, owner))
}

fn record(version: ComponentSchemaVersion, declared_owner: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        version,
        ComponentValue::map([("value", ComponentValue::U64(1))]),
    )
    .with_owner(owner(declared_owner))
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new(COMPONENT)
}
fn owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}
fn source(value: &str) -> AuthoringDocumentSource {
    AuthoringDocumentSource::new(AssetRef::path(value).unwrap())
}
