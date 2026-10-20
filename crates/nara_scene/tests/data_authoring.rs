#![cfg(feature = "serde")]

use std::collections::BTreeSet;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{AuthoringAdapterReceipt, AuthoringPreviewOmissionReason},
    AuthoringDocumentSource, AuthoringRecordReadiness, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation,
};

#[path = "support/vec2_data_authoring.rs"]
mod vec2_data_authoring;

fn bundle() -> AuthoringSchemaBundle {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("amount"),
        "Amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValueKind::U64,
    )
    .with_numeric_domain(Some(ComponentNumericDomain::U8))
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.config").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("game.Config"),
                    "Configuration",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([field]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn document(value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("player").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("game.Config"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )
    .with_component(
        ComponentTypeId::new("game.Missing"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("retain me".into()),
        ),
    )])
}

fn source() -> AuthoringDocumentSource {
    AuthoringDocumentSource::new(AssetRef::path("scenes/custom.scene.json").unwrap())
}

fn patch(amount: u64) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: SceneEntityId::new("player").unwrap(),
        component: ComponentTypeId::new("game.Config"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("amount"),
        value: ComponentValue::U64(amount),
    }])
}

#[test]
fn data_adapter_edits_scene_and_prefab_without_native_preview_or_unavailable_mutation() {
    let bundle = bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    assert!(adapter.contribution().is_none());
    let document = document(ComponentValue::map([("amount", ComponentValue::U64(7))]));
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter.clone()])
            .unwrap()
            .publish();
    let fact = published
        .record_facts()
        .iter()
        .find(|fact| fact.component().as_str() == "game.Config")
        .unwrap();
    assert_eq!(fact.readiness(), AuthoringRecordReadiness::Available);
    assert!(!fact.adapter().unwrap().supports_native_preview());
    let preview = nara_scene::__private::project_published_authoring_scene_preview(&published);
    assert!(preview.document().entities[0].components.is_empty());
    assert!(
        preview.omissions().iter().any(|omission| omission.reason()
            == AuthoringPreviewOmissionReason::NativeBindingUnavailable)
    );
    let manifest = published.unavailable_manifest().clone();
    assert_eq!(manifest.record_count(), 1);
    let mut scene = SceneAuthoringSession::from_published_authoring_document(published);
    let original = scene.document().clone();
    assert!(scene.apply_authoring_patch(&patch(255)).applied);
    let accepted = scene.document().clone();
    let revision = scene.revision();
    let history = scene.history_status();
    assert!(!scene.apply_authoring_patch(&patch(256)).applied);
    assert_eq!(scene.document(), &accepted);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.history_status(), history);
    assert_eq!(scene.unavailable_record_manifest(), Some(&manifest));
    assert!(scene.undo_authoring().applied);
    assert_eq!(scene.document(), &original);
    assert!(scene.redo_authoring().applied);
    let reopened = SceneDocumentCandidate::decode_json_bytes(
        scene.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap()
    .admit_authoring(source(), &bundle, &[adapter.clone()])
    .unwrap()
    .publish();
    assert_eq!(reopened.document(), &accepted);
    assert_eq!(reopened.unavailable_manifest(), &manifest);

    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let prefab_manifest = published.unavailable_manifest().clone();
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    let original = prefab.document().clone();
    assert!(prefab.apply_authoring_patch(&patch(8)).applied);
    let accepted = prefab.document().clone();
    let revision = prefab.revision();
    assert!(!prefab.apply_authoring_patch(&patch(256)).applied);
    assert_eq!(prefab.document(), &accepted);
    assert_eq!(prefab.revision(), revision);
    assert!(prefab.undo_authoring().applied);
    assert_eq!(prefab.document(), &original);
    assert!(prefab.redo_authoring().applied);
    assert_eq!(prefab.unavailable_record_manifest(), Some(&prefab_manifest));
}

#[test]
fn data_adapter_rejects_invalid_initial_values_and_stale_catalog_coverage() {
    let bundle = bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    for value in [
        ComponentValue::map([("amount", ComponentValue::U64(256))]),
        ComponentValue::map([("amount", ComponentValue::String("7".into()))]),
        ComponentValue::map([("amount", ComponentValue::Null)]),
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
        ComponentValue::map([
            ("amount", ComponentValue::U64(7)),
            ("undeclared", ComponentValue::Bool(true)),
        ]),
    ] {
        let document = document(value);
        assert!(
            SceneDocumentCandidate::decode_json_bytes(
                document.to_json_string().unwrap().as_bytes()
            )
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter.clone()])
            .is_err()
        );
    }
    let mut changed = bundle.owners()[0].current().clone();
    changed.components[0].fields[0]
        .aliases
        .push("Changed identity".into());
    let changed = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        bundle.owners()[0].owner_id().clone(),
        changed,
        None,
    )])
    .unwrap();
    let document = document(ComponentValue::map([("amount", ComponentValue::U64(7))]));
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &changed, &[adapter])
            .unwrap()
            .publish();
    assert_eq!(
        published
            .record_facts()
            .iter()
            .find(|fact| fact.component().as_str() == "game.Config")
            .unwrap()
            .readiness(),
        AuthoringRecordReadiness::KnownUnbound
    );
}

#[test]
fn optional_null_scalar_edits_have_reversible_scene_and_prefab_history() {
    let required = bundle();
    let mut catalog = required.owners()[0].current().clone();
    catalog.components[0].fields[0].required = false;
    catalog.components[0].fields[0].shape =
        nara_reflect::ComponentValueShape::optional(catalog.components[0].fields[0].shape.clone());
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        required.owners()[0].owner_id().clone(),
        catalog,
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    let document = document(ComponentValue::map([("amount", ComponentValue::Null)]));
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter.clone()])
            .unwrap()
            .publish();
    let mut scene = SceneAuthoringSession::from_published_authoring_document(published);
    let original = scene.document().clone();
    assert!(scene.apply_authoring_patch(&patch(9)).applied);
    let accepted = scene.document().clone();
    assert!(scene.undo_authoring().applied);
    assert_eq!(scene.document(), &original);
    assert!(scene.redo_authoring().applied);
    assert_eq!(scene.document(), &accepted);

    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(published);
    let original = prefab.document().clone();
    assert!(prefab.apply_authoring_patch(&patch(9)).applied);
    let accepted = prefab.document().clone();
    assert!(prefab.undo_authoring().applied);
    assert_eq!(prefab.document(), &original);
    assert!(prefab.redo_authoring().applied);
    assert_eq!(prefab.document(), &accepted);
}

#[test]
fn data_adapter_requires_complete_finite_coverage_and_current_versions() {
    let original = bundle();
    let owner = &original.owners()[0];
    let covered = BTreeSet::from([ComponentTypeId::new("game.Config")]);
    assert!(AuthoringAdapterReceipt::from_schema_owner(owner, &covered).is_none());
    for fields in [
        Vec::new(),
        vec![
            owner.current().components[0].fields[0].clone(),
            ComponentFieldSchema::required(
                ComponentFieldId::new("complex"),
                "Complex",
                ComponentFieldPath::from_fields(["complex"]),
                ComponentValueKind::Map,
            ),
        ],
    ] {
        let mut catalog = owner.current().clone();
        catalog.components[0].fields = fields;
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.owner_id().clone(),
            catalog,
            None,
        )])
        .unwrap();
        assert!(
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new())
                .is_none()
        );
    }
    let adapter = AuthoringAdapterReceipt::from_schema_owner(owner, &BTreeSet::new()).unwrap();
    let mut future = document(ComponentValue::map([("amount", ComponentValue::U64(7))]));
    future.entities[0]
        .components
        .get_mut(&ComponentTypeId::new("game.Config"))
        .unwrap()
        .version = ComponentSchemaVersion::new(2).unwrap();
    let published =
        SceneDocumentCandidate::decode_json_bytes(future.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &original, &[adapter])
            .unwrap()
            .publish();
    assert_eq!(
        published
            .record_facts()
            .iter()
            .find(|fact| fact.component().as_str() == "game.Config")
            .unwrap()
            .readiness(),
        AuthoringRecordReadiness::FutureSchema
    );
    assert_eq!(published.unavailable_manifest().record_count(), 2);
}
