#![cfg(feature = "cook")]

use std::collections::BTreeMap;

use bevy_ecs::{component::Component, hierarchy::ChildOf, world::World};
use nara_core::{AssetId, Color, DocumentId, EntityUuid, Vec2, vfs::AssetPath};
use nara_reflect::{
    ComponentCodecError, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueRef, PackedU16, Schema, TypeSchema, validate_component_value,
};
use nara_scene::{
    Name,
    authoring::{
        PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument, SceneEntity,
        builtin_component_schemas, prefab::expand_prefabs,
    },
    components::{ColliderSource, SpriteSource, Transform2d, Visibility},
    cook::{CookSceneError, cook_flattened_scene, cook_scene},
    cooked::{CookedRegistry, CookedScene, CookedSceneError},
    spatial::{GlobalTransform2d, InheritedVisibility},
};
use uuid::Uuid;

#[derive(Component, Schema, Debug, PartialEq)]
#[schema(crate = "nara_reflect", id = "game.CookedGrid", version = 2)]
struct Grid {
    enabled: bool,
    target: EntityUuid,
    masks: BTreeMap<String, Vec<Option<AssetId>>>,
    tile_cells: PackedU16,
}

#[derive(Component, Schema)]
#[schema(crate = "nara_reflect", id = "game.CookedGrid", version = 3)]
struct IncompatibleGrid {
    enabled: bool,
}

fn id(value: u128) -> EntityUuid {
    EntityUuid::try_from_uuid(Uuid::from_u128(value)).unwrap()
}

fn asset(value: u128) -> AssetId {
    AssetId::try_from_uuid(Uuid::from_u128(value)).unwrap()
}

fn document(value: u128, entities: Vec<SceneEntity>) -> SceneDocument {
    SceneDocument {
        id: DocumentId::try_from_uuid(Uuid::from_u128(value)).unwrap(),
        assets: BTreeMap::new(),
        entities,
    }
}

fn catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog::from_components([Grid::type_schema().unwrap()]).unwrap()
}

fn registry() -> CookedRegistry {
    let mut registry = CookedRegistry::new();
    registry
        .register_builtins()
        .unwrap()
        .register::<Grid>()
        .unwrap();
    registry
}

fn grid(target: EntityUuid) -> Grid {
    Grid {
        enabled: true,
        target,
        masks: BTreeMap::from([("mask".to_owned(), vec![Some(asset(500)), None])]),
        tile_cells: vec![7, 42, u16::MAX].into(),
    }
}

#[test]
fn handwritten_schema_decoders_reject_undeclared_keys() {
    let visibility = ComponentValue::map([
        ("mode", ComponentValue::String("visible".into())),
        ("unexpected", ComponentValue::Bool(true)),
    ]);
    let name = ComponentValue::map([
        ("value", ComponentValue::String("Player".into())),
        ("unexpected", ComponentValue::Bool(true)),
    ]);
    for error in [
        Visibility::decode_component((&visibility).into()).unwrap_err(),
        Name::decode_component((&name).into()).unwrap_err(),
    ] {
        assert!(
            matches!(error, ComponentCodecError::Message(message) if message.contains("'unexpected'"))
        );
    }
    for value in [
        ComponentValue::Map(BTreeMap::new()),
        ComponentValue::map([("mode", ComponentValue::Null)]),
    ] {
        assert_eq!(
            Visibility::decode_component((&value).into()).unwrap(),
            Visibility::Inherited
        );
    }
    let name = Name::new("Player");
    let value = name.encode_component().unwrap();
    assert_eq!(Name::decode_component((&value).into()).unwrap(), name);
}

#[test]
fn prefab_cook_is_deterministic_and_matches_the_native_authored_components() {
    let mut parent = SceneEntity::new(id(11), "Prefab Root");
    parent.set(&Transform2d::from_xy(1.0, 3.0)).unwrap();
    parent.set(&grid(id(12))).unwrap();
    parent.set(&Visibility::Hidden).unwrap();
    parent
        .set(&SpriteSource {
            size: Vec2::new(4.0, 2.0),
            tint: Color::rgba(0.9, 0.35, 0.1, 1.0),
            image: Some(asset(300)),
            material: Some(asset(250)),
            ..Default::default()
        })
        .unwrap();
    let mut child = SceneEntity::new(id(12), "Prefab Child");
    child.parent = Some(parent.id);
    child.components.insert(
        ComponentTypeId::new(Visibility::COMPONENT_ID),
        SceneComponent::known(
            Visibility::type_schema().unwrap().version(),
            ComponentValue::map([("mode", ComponentValue::Null)]),
        ),
    );
    child
        .set(&ColliderSource {
            radius: Some(4.5),
            ..Default::default()
        })
        .unwrap();
    let mut prefab = document(100, vec![child, parent]);
    prefab.assets.insert(
        "gate".into(),
        SceneAsset {
            id: asset(250),
            hint: AssetPath::new("assets/gate.material.ron").unwrap(),
        },
    );
    prefab.assets.insert(
        "unused".into(),
        SceneAsset {
            id: asset(17),
            hint: AssetPath::new("assets/unused.png").unwrap(),
        },
    );
    let schemas = catalog();
    let prefab_ron = prefab.to_ron_with(schemas.components()).unwrap();
    assert!(prefab_ron.contains("material: \"@gate\""));
    prefab = SceneDocument::parse_with(&prefab_ron, schemas.components()).unwrap();
    let mut mount = SceneEntity::new(id(1), "Room");
    mount.parent = Some(id(2));
    let mut instance = PrefabInstance::new(asset(220));
    instance.overrides.insert(
        "nara.Transform2d.translation".into(),
        RawRon::from_ron("(5.0, -2.0)").unwrap(),
    );
    mount.prefab = Some(instance);
    let mut source = document(200, vec![SceneEntity::new(id(2), "Scene root"), mount]);

    let load = |id| {
        (id == asset(220))
            .then(|| prefab.clone())
            .ok_or("missing prefab")
    };
    let output = cook_scene(&source, &schemas, load).unwrap();
    let full = ComponentSchemaCatalog::from_components(
        builtin_component_schemas()
            .unwrap()
            .into_iter()
            .chain(schemas.components().iter().cloned()),
    )
    .unwrap();
    let flattened = expand_prefabs(&source, &full, load).unwrap().document;
    source.entities.reverse();
    prefab.entities.reverse();
    let reordered = cook_scene(&source, &full, |_| Ok::<_, &str>(prefab.clone())).unwrap();
    assert_eq!(output.bytes, reordered.bytes);
    assert_eq!(
        output.bytes,
        cook_flattened_scene(&flattened, &full).unwrap().bytes
    );
    assert_eq!(output.prefab_dependencies, [asset(220)]);
    assert_eq!(
        output.asset_dependencies,
        [asset(250), asset(300), asset(500)]
    );
    assert_eq!(&output.bytes[..16], b"NSCN\x01\0\0\0\x03\0\0\0\x06\0\0\0");

    let cooked = CookedScene::parse(&output.bytes).unwrap();
    assert_eq!(cooked.asset_dependencies(), output.asset_dependencies);
    let mut world = World::new();
    let spawned = cooked.instantiate(&mut world, &registry()).unwrap();
    assert_eq!(spawned.len(), 3);
    for row in flattened.entities {
        let entity = world.entity(spawned[&row.id]);
        assert_eq!(entity.get::<Name>().unwrap().as_str(), row.name);
        assert_eq!(
            entity.get::<Transform2d>().cloned(),
            row.typed::<Transform2d>().unwrap()
        );
        assert_eq!(
            entity.get::<Visibility>().cloned(),
            row.typed::<Visibility>().unwrap()
        );
        assert_eq!(
            entity.get::<SpriteSource>().cloned(),
            row.typed::<SpriteSource>().unwrap()
        );
        assert_eq!(
            entity.get::<ColliderSource>().cloned(),
            row.typed::<ColliderSource>().unwrap()
        );
        assert_eq!(entity.get::<Grid>(), row.typed::<Grid>().unwrap().as_ref());
        assert_eq!(
            entity.get::<ChildOf>().map(ChildOf::parent),
            row.parent.map(|parent| spawned[&parent])
        );
    }
    let room = world.entity(spawned[&id(1)]);
    assert_eq!(
        room.get::<Transform2d>().unwrap().translation,
        Vec2::new(5.0, -2.0)
    );
    assert!(room.contains::<GlobalTransform2d>() && room.contains::<InheritedVisibility>());
    let grid = room.get::<Grid>().unwrap();
    assert_eq!(
        world.get::<Name>(spawned[&grid.target]).unwrap().as_str(),
        "Prefab Child"
    );
}

fn two_rows() -> SceneDocument {
    let mut leaf = SceneEntity::new(id(3), "Leaf");
    leaf.parent = Some(id(4));
    leaf.set(&grid(id(4))).unwrap();
    document(201, vec![SceneEntity::new(id(4), "Root"), leaf])
}

#[test]
fn rejects_broken_headers_identity_tables_and_truncation() {
    let output = cook_flattened_scene(&two_rows(), &catalog()).unwrap();
    let bytes = output.bytes;
    for prefix in 0..bytes.len() {
        assert!(
            CookedScene::parse(&bytes[..prefix]).is_err(),
            "prefix {prefix}"
        );
    }
    let mut changed = bytes.clone();
    changed[4] = 2;
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::UnsupportedVersion(2))
    ));
    changed[4] = 1;
    changed[6] = 1;
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::UnsupportedFlags(1))
    ));
    changed.copy_from_slice(&bytes);
    changed[16] ^= 1;
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::SchemaDigestMismatch)
    ));
    changed.copy_from_slice(&bytes);
    let first_asset = 36;
    changed[first_asset..first_asset + 16].fill(0);
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::Format("nil asset UUID"))
    ));
    changed.copy_from_slice(&bytes);
    let first_parent = first_asset + 16 + 16;
    changed[first_parent..first_parent + 4].copy_from_slice(&2_i32.to_le_bytes());
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::InvalidParent { .. })
    ));
    changed[first_parent..first_parent + 4].copy_from_slice(&0_i32.to_le_bytes());
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::HierarchyCycle(_))
    ));
    changed.copy_from_slice(&bytes);
    let first_column = first_asset + 16 + 2 * 20;
    changed[first_column + 48..first_column + 52].copy_from_slice(&2_u32.to_le_bytes());
    assert!(matches!(
        CookedScene::parse(&changed),
        Err(CookedSceneError::Format(
            "invalid column entity order or index"
        ))
    ));
}

#[test]
fn invalid_typed_payloads_and_incompatible_registries_leave_existing_world_intact() {
    let output = cook_flattened_scene(&two_rows(), &catalog()).unwrap();
    let mut world = World::new();
    let existing = world.spawn(Name::new("existing")).id();
    let initial_count = world.entity_count();
    let mut wrong = CookedRegistry::new();
    wrong
        .register_builtins()
        .unwrap()
        .register::<IncompatibleGrid>()
        .unwrap();
    let scene = CookedScene::parse(&output.bytes).unwrap();
    assert!(matches!(
        scene.instantiate(&mut world, &wrong),
        Err(CookedSceneError::SchemaMismatch {
            component: "game.CookedGrid"
        })
    ));
    let mut incomplete = CookedRegistry::new();
    incomplete.register_builtins().unwrap();
    assert!(matches!(
        scene.instantiate(&mut world, &incomplete),
        Err(CookedSceneError::MissingSchema { .. })
    ));
    assert!(matches!(
        wrong.register::<Grid>(),
        Err(CookedSceneError::DuplicateType { .. })
    ));

    let grid_payload = grid(id(4)).encode_postcard().unwrap();
    let payload_at = output
        .bytes
        .windows(grid_payload.len())
        .position(|candidate| candidate == grid_payload)
        .unwrap();
    let mut changed = output.bytes.clone();
    changed[payload_at] = 2;
    let scene = CookedScene::parse(&changed).unwrap();
    assert!(matches!(
        scene.instantiate(&mut world, &registry()),
        Err(CookedSceneError::Component {
            component: "game.CookedGrid",
            ..
        })
    ));
    changed.copy_from_slice(&output.bytes);
    changed[payload_at + 1..payload_at + 17].copy_from_slice(id(10).as_uuid().as_bytes());
    let scene = CookedScene::parse(&changed).unwrap();
    assert!(
        matches!(scene.instantiate(&mut world, &registry()), Err(CookedSceneError::MissingEntityReference { target, .. }) if target == id(10))
    );
    changed.copy_from_slice(&output.bytes);
    changed[36..52].copy_from_slice(asset(51).as_uuid().as_bytes());
    let scene = CookedScene::parse(&changed).unwrap();
    assert!(
        matches!(scene.instantiate(&mut world, &registry()), Err(CookedSceneError::MissingAssetReference { asset: missing, .. }) if missing == asset(500))
    );

    assert_eq!(world.entity_count(), initial_count);
    assert_eq!(world.get::<Name>(existing).unwrap().as_str(), "existing");
    assert!(world.component_id::<Grid>().is_none());
}

#[test]
fn cook_requires_resolved_prefabs_schemas_and_entity_references() {
    let mut source = two_rows();
    let serialized = source.to_ron_with(catalog().components()).unwrap();
    let opaque = SceneDocument::parse(&serialized).unwrap();
    assert!(matches!(
        cook_flattened_scene(&opaque, &catalog()),
        Err(CookSceneError::Opaque { component, .. }) if component.as_str() == Grid::COMPONENT_ID
    ));
    source.entities[0].prefab = Some(PrefabInstance::new(asset(123)));
    assert!(matches!(
        cook_flattened_scene(&source, &catalog()),
        Err(CookSceneError::UnexpandedPrefab(_))
    ));
    source.entities[0].prefab = None;
    source.entities[1].set(&grid(id(9))).unwrap();
    assert!(
        matches!(cook_flattened_scene(&source, &catalog()), Err(CookSceneError::MissingEntityReference { target, .. }) if target == id(9))
    );
    assert!(matches!(
        cook_flattened_scene(&source, &ComponentSchemaCatalog::default()),
        Err(CookSceneError::MissingSchema { .. })
    ));

    let empty = document(100, vec![]);
    let output = cook_flattened_scene(&empty, &catalog()).unwrap();
    assert_eq!(output.bytes.len(), 36);
    let scene = CookedScene::parse(&output.bytes).unwrap();
    assert!(
        scene
            .instantiate(&mut World::new(), &CookedRegistry::new())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn source_only_schema_is_rejected_for_cook_and_native_registration() {
    #[derive(Component, Debug, PartialEq)]
    struct SourceOnly;

    impl Schema for SourceOnly {
        const COMPONENT_ID: &'static str = "game.SourceOnly";

        fn type_schema() -> Result<TypeSchema, ComponentCodecError> {
            Ok(TypeSchema::new(
                ComponentTypeId::new(Self::COMPONENT_ID),
                "Source Only",
                ComponentSchemaVersion::ONE,
            ))
        }

        fn encode_component(&self) -> Result<ComponentValue, ComponentCodecError> {
            Ok(ComponentValue::Map(BTreeMap::new()))
        }

        fn decode_component(value: ComponentValueRef<'_>) -> Result<Self, ComponentCodecError> {
            validate_component_value(&Self::type_schema()?, value)?;
            Ok(Self)
        }
    }

    let schemas =
        ComponentSchemaCatalog::from_components([SourceOnly::type_schema().unwrap()]).unwrap();
    let entity = id(1);
    let mut source = document(100, vec![SceneEntity::new(entity, "Authored")]);
    assert!(cook_flattened_scene(&source, &schemas).is_ok());
    source.entities[0].set(&SourceOnly).unwrap();
    let ron = source.to_ron_with(schemas.components()).unwrap();
    let source = SceneDocument::parse_with(&ron, schemas.components()).unwrap();
    assert_eq!(
        source.entities[0].typed::<SourceOnly>().unwrap(),
        Some(SourceOnly)
    );

    assert!(matches!(
        cook_flattened_scene(&source, &schemas),
        Err(CookSceneError::Component {
            entity: row,
            component,
            source: ComponentCodecError::PostcardUnavailable,
        }) if row == entity && component.as_str() == SourceOnly::COMPONENT_ID
    ));
    assert!(matches!(
        CookedRegistry::new().register::<SourceOnly>(),
        Err(CookedSceneError::Schema {
            component: SourceOnly::COMPONENT_ID,
            source: ComponentCodecError::PostcardUnavailable,
        })
    ));
}
