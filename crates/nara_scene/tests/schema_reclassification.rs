#![cfg(feature = "serde")]

use std::sync::Arc;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentReadiness, AuthoringDocumentSource,
    AuthoringRecordReadiness, AuthoringSchemaBasis, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, PreparedPrefabSchemaReclassification,
    PreparedSceneSchemaReclassification, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner, SchemaReclassificationWork,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

#[test]
fn independent_consumer_recovers_scene_and_prefab_with_shared_work_and_exact_source_identity() {
    let previous =
        AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let component = ComponentTypeId::new("consumer.Recovered");
    let unknown = ComponentTypeId::new("missing.Unrelated");
    let owner = PersistentComponentSchemaOwnerId::try_new("consumer.recovered").unwrap();
    let schema = ComponentSchema::new(component.clone(), "Recovered", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let next = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("root"))
        .with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::U64(41))]),
            )
            .with_owner(owner),
        )
        .with_component(
            unknown.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("opaque unrelated data".into()),
            ),
        )]);
    let mut scene = SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("scenes/consumer.json").unwrap()),
                &previous,
            )
            .unwrap()
            .publish(),
    );
    let prefab_document = PrefabDocument::new(document.roots, document.entities);
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_str(&prefab_document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/consumer.json").unwrap()),
                &previous,
            )
            .unwrap()
            .publish(),
    );
    let scene_document = scene.shared_document();
    let prefab_document = prefab.shared_document();
    let scene_before = (
        scene.revision(),
        scene.content_state(),
        scene.history_status(),
        scene.history_usage(),
        scene.source_upgrade_required(),
        scene.authoring_source().cloned(),
    );
    let prefab_before = (
        prefab.revision(),
        prefab.content_state(),
        prefab.history_status(),
        prefab.history_usage(),
        prefab.source_upgrade_required(),
        prefab.authoring_source().cloned(),
    );
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id("pending")),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let scene_edit = scene
        .prepare_authoring_edit(scene.revision(), &patch)
        .unwrap();
    let prefab_edit = prefab
        .prepare_authoring_edit(prefab.revision(), &patch)
        .unwrap();
    let mut work = SchemaReclassificationWork::default();
    let prepared_scene: PreparedSceneSchemaReclassification = scene
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    let first_usage = work.usage();
    let prepared_prefab: PreparedPrefabSchemaReclassification = prefab
        .prepare_schema_reclassification(&next, &mut work)
        .unwrap();
    assert!(work.usage().work_bytes > first_usage.work_bytes);
    assert_eq!(prepared_scene.base_revision(), scene_before.0);
    assert_eq!(prepared_prefab.base_revision(), prefab_before.0);
    assert_eq!(prepared_scene.schema_basis(), next);
    assert_eq!(prepared_prefab.schema_basis(), next);
    assert_eq!(
        prepared_scene.readiness(),
        AuthoringDocumentReadiness::Degraded
    );
    assert_eq!(
        prepared_prefab.readiness(),
        AuthoringDocumentReadiness::Degraded
    );
    let scene_manifest = prepared_scene.unavailable_record_manifest().clone();
    let prefab_manifest = prepared_prefab.unavailable_record_manifest().clone();
    assert_eq!(scene_manifest.total_count(), 1);
    assert_eq!(prefab_manifest.total_count(), 1);
    assert!(!prepared_scene.diagnostics().has_errors());
    assert!(!prepared_prefab.diagnostics().has_errors());
    assert_eq!(scene.authoring_schema_basis(), Some(previous.clone()));
    assert_eq!(prefab.authoring_schema_basis(), Some(previous));
    let scene_commit = scene.bind_schema_reclassification(prepared_scene).unwrap();
    let prefab_commit = prefab
        .bind_schema_reclassification(prepared_prefab)
        .unwrap();
    scene_commit.commit();
    prefab_commit.commit();
    assert!(!scene.commit_authoring_edit(scene_edit).applied);
    assert!(!prefab.commit_authoring_edit(prefab_edit).applied);
    assert!(Arc::ptr_eq(&scene_document, &scene.shared_document()));
    assert!(Arc::ptr_eq(&prefab_document, &prefab.shared_document()));
    assert_eq!(
        scene_before,
        (
            scene.revision(),
            scene.content_state(),
            scene.history_status(),
            scene.history_usage(),
            scene.source_upgrade_required(),
            scene.authoring_source().cloned(),
        )
    );
    assert_eq!(
        prefab_before,
        (
            prefab.revision(),
            prefab.content_state(),
            prefab.history_status(),
            prefab.history_usage(),
            prefab.source_upgrade_required(),
            prefab.authoring_source().cloned(),
        )
    );
    assert_eq!(scene.authoring_schema_basis(), Some(next.clone()));
    assert_eq!(prefab.authoring_schema_basis(), Some(next));
    assert_eq!(scene.unavailable_record_manifest(), Some(&scene_manifest));
    assert_eq!(prefab.unavailable_record_manifest(), Some(&prefab_manifest));
    for (readiness, facts) in [
        (
            scene.authoring_readiness(),
            scene.authoring_record_facts().unwrap(),
        ),
        (
            prefab.authoring_readiness(),
            prefab.authoring_record_facts().unwrap(),
        ),
    ] {
        assert_eq!(readiness, Some(AuthoringDocumentReadiness::Degraded));
        assert_eq!(facts.len(), 2);
        let recovered = facts
            .iter()
            .find(|fact| fact.component() == &component)
            .unwrap();
        assert_eq!(recovered.readiness(), AuthoringRecordReadiness::Available);
        assert!(!recovered.adapter().unwrap().supports_native_preview());
        let unavailable = facts
            .iter()
            .find(|fact| fact.component() == &unknown)
            .unwrap();
        assert_eq!(
            unavailable.readiness(),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert!(unavailable.adapter().is_none());
    }
    assert!(
        scene
            .replace_document(scene_document.as_ref().clone())
            .is_err()
    );
    assert!(
        prefab
            .replace_document(prefab_document.as_ref().clone())
            .is_err()
    );
}
