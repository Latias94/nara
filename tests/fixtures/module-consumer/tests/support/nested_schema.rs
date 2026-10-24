use std::collections::BTreeMap;

use nara_asset::{
    AssetPath, AssetRecord, AssetRef, AssetSourceKind, ProjectAssetDatabase, StableAssetId,
};
use nara_ecs::Component;
use nara_identity::EntityReference;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentRegistry, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, PersistentComponent, PersistentComponentProvider,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    register_scene_components,
};

pub const COMPONENT_ID: &str = "consumer.nested.Relationships";
pub const PREFAB_PATH: &str = "prefabs/relationships.prefab.ron";
pub const PREFAB_ID: &str = "70bd3b0f-f4d3-486c-9566-6cb61e7c4c65";
pub const IMAGE_PATH: &str = "textures/portrait.png";
pub const IMAGE_ID: &str = "fabfe185-a67e-43c2-98cd-d6f143a08e75";

pub type TargetGroups = BTreeMap<String, Vec<Option<EntityReference>>>;

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "consumer.nested.Relationships",
    version = 1,
    alias = "Relationships",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Relationships {
    #[nara(
        id = "targets",
        alias = "Targets",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    pub targets: TargetGroups,
    #[nara(
        id = "portraits",
        alias = "Portraits",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    pub portraits: Vec<Option<AssetRef>>,
    #[nara(id = "weights", alias = "Weights")]
    pub weights: Vec<u8>,
}

pub fn relationships() -> Relationships {
    Relationships {
        targets: BTreeMap::from([(
            "friends".to_owned(),
            vec![
                Some(EntityReference::SceneLocal {
                    entity: SceneEntityId::new("friend").unwrap(),
                }),
                None,
            ],
        )]),
        portraits: vec![Some(AssetRef::stable_id(IMAGE_ID).unwrap()), None],
        weights: vec![1, 2],
    }
}

pub fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("actor").unwrap()).with_component(
            ComponentTypeId::new(COMPONENT_ID),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                relationships().__encode_persistent_component().unwrap(),
            ),
        ),
        SceneEntityRecord::new(SceneEntityId::new("friend").unwrap()),
    ])
}

pub fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry
        .register_persistent_component::<Relationships>()
        .unwrap();
    registry.freeze().unwrap();
    registry
}

pub fn bundle() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("consumer.nested").unwrap(),
        ComponentSchemaCatalog {
            components: vec![Relationships::persistent_component_schema()],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

pub fn database() -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    for (id, path, kind) in [
        (IMAGE_ID, IMAGE_PATH, AssetSourceKind::Image),
        (PREFAB_ID, PREFAB_PATH, AssetSourceKind::Prefab),
    ] {
        database
            .insert(AssetRecord::new(
                StableAssetId::parse_str(id).unwrap(),
                AssetPath::new(path).unwrap(),
                kind,
            ))
            .unwrap();
    }
    database
}
