use std::collections::BTreeMap;

use nara_reflect::{ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::{
    PrefabAuthoringSession, PrefabDocument, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneEntityId, ScenePlacement, ScenePlacementOwner, register_scene_components,
};

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    registry
}

fn components() -> BTreeMap<ComponentTypeId, SceneComponentRecord> {
    BTreeMap::from([(
        ComponentTypeId::new("nara.scene.Name"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("Made".into()),
        ),
    )])
}

#[test]
fn native_creation_keeps_project_database_codec_admission_for_both_source_kinds() {
    use nara_asset::ProjectAssetDatabase;
    use nara_ecs::Component;
    use nara_reflect::{
        ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentSchema, ComponentValueKind, PreparedComponentCandidate,
    };

    #[derive(Component)]
    struct DatabaseBound;

    let component = ComponentTypeId::new("consumer.DatabaseBound");
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_codec_with_context::<DatabaseBound, _, _>(
            ComponentSchema::new(
                component.clone(),
                "Database bound",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::empty(),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            |_, context| {
                context.asset_resolution().ok_or_else(|| {
                    ComponentCodecError::Message("project database required".into())
                })?;
                Ok(PreparedComponentCandidate::insert(DatabaseBound))
            },
            |_, _, _| Ok(ComponentValue::String("bound".into())),
        )
        .unwrap();
    registry.freeze().unwrap();
    let components = || {
        BTreeMap::from([(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("bound".into()),
            ),
        )])
    };
    let database = ProjectAssetDatabase::default();
    let placement = ScenePlacement::last(ScenePlacementOwner::Root);
    let mut scene = SceneAuthoringSession::new(SceneDocument::default());
    assert!(
        scene
            .create_entity_with_registry(
                scene.revision(),
                &registry,
                placement.clone(),
                components()
            )
            .is_err()
    );
    assert!(scene.document().entities.is_empty());
    let created = scene
        .create_entity_with_asset_resolution(
            scene.revision(),
            &registry,
            database.resolver(&nara_asset::AssetPathScope::ROOT),
            placement.clone(),
            components(),
        )
        .unwrap();
    let entity = created.entity().clone();
    assert!(
        scene
            .undo_with_asset_resolution(
                &registry,
                database.resolver(&nara_asset::AssetPathScope::ROOT)
            )
            .applied
    );
    assert!(scene.document().entities.is_empty());
    assert!(
        scene
            .redo_with_asset_resolution(
                &registry,
                database.resolver(&nara_asset::AssetPathScope::ROOT)
            )
            .applied
    );
    assert_eq!(scene.document().roots, [entity.clone()]);
    // Even empty creation must validate the database-bound records already in the source.
    assert!(
        scene
            .create_entity_with_asset_resolution(
                scene.revision(),
                &registry,
                database.resolver(&nara_asset::AssetPathScope::ROOT),
                placement.clone(),
                BTreeMap::new()
            )
            .is_ok()
    );

    let mut prefab = PrefabAuthoringSession::new(PrefabDocument::default());
    assert!(
        prefab
            .create_entity_with_registry(
                prefab.revision(),
                &registry,
                placement.clone(),
                components()
            )
            .is_err()
    );
    let created = prefab
        .create_entity_with_asset_resolution(
            prefab.revision(),
            &registry,
            database.resolver(&nara_asset::AssetPathScope::ROOT),
            placement,
            components(),
        )
        .unwrap();
    let prefab_entity = created.entity().clone();
    assert!(
        prefab
            .undo_with_asset_resolution(
                &registry,
                database.resolver(&nara_asset::AssetPathScope::ROOT)
            )
            .applied
    );
    assert!(prefab.document().entities.is_empty());
    assert!(
        prefab
            .redo_with_asset_resolution(
                &registry,
                database.resolver(&nara_asset::AssetPathScope::ROOT)
            )
            .applied
    );
    assert_eq!(prefab.document().roots, [prefab_entity]);
}

#[test]
fn native_source_owners_create_and_replay_the_exact_returned_identity() {
    let registry = registry();
    let placement = ScenePlacement::last(ScenePlacementOwner::Root);
    let mut scene = SceneAuthoringSession::new(SceneDocument::default());
    let created = scene
        .create_entity_with_registry(scene.revision(), &registry, placement.clone(), components())
        .unwrap();
    let first = created.entity().clone();
    assert!(created.report().applied);
    assert_eq!(scene.document().roots, [first.clone()]);
    assert!(first.as_str().len() <= 22);
    assert_eq!(scene.document().entities[0].components.len(), 1);
    assert!(scene.undo(&registry).applied);
    assert!(scene.document().entities.is_empty());
    assert!(scene.redo(&registry).applied);
    assert_eq!(scene.document().roots, [first.clone()]);
    let (_, report) = created.into_parts();
    assert!(report.applied);

    let mut prefab = PrefabAuthoringSession::new(PrefabDocument::default());
    let created = prefab
        .create_entity_with_registry(prefab.revision(), &registry, placement, components())
        .unwrap();
    let second = created.entity().clone();
    assert_ne!(first, second);
    assert_eq!(prefab.document().roots, [second.clone()]);
    assert!(prefab.undo(&registry).applied);
    assert!(prefab.document().entities.is_empty());
    assert!(prefab.redo(&registry).applied);
    assert_eq!(prefab.document().roots, [second]);
}

#[test]
fn native_creation_refuses_stale_basis_and_final_placement_without_publication() {
    let registry = registry();
    let mut owner = SceneAuthoringSession::new(SceneDocument::default());
    let revision = owner.revision();
    let history = owner.history_status();
    let content = owner.content_state();
    assert!(
        owner
            .create_entity_with_registry(
                revision.next(),
                &registry,
                ScenePlacement::last(ScenePlacementOwner::Root),
                components(),
            )
            .is_err()
    );
    assert!(
        owner
            .create_entity_with_registry(
                revision,
                &registry,
                ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: SceneEntityId::new("missing").unwrap(),
                }),
                components(),
            )
            .is_err()
    );
    assert_eq!(owner.document(), &SceneDocument::default());
    assert_eq!(owner.revision(), revision);
    assert_eq!(owner.history_status(), history);
    assert_eq!(owner.content_state(), content);
    assert!(
        owner
            .create_entity_with_registry(
                revision,
                &registry,
                ScenePlacement::last(ScenePlacementOwner::Root),
                components(),
            )
            .is_ok()
    );
}
