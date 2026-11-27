#![cfg(feature = "authoring")]

use std::collections::BTreeMap;

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_reflect::{
    AssetReferenceDemand, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeError, ComponentShapeErrorKind, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
};
use nara_scene::authoring::{
    CompactValueError, PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument,
    SceneEntity, UnknownComponent,
    prefab::{
        ExpandedScene, MAX_EXPANDED_ENTITIES, MAX_PREFAB_DEPTH, PrefabError, PrefabOverrideError,
        PrefabOverridePath, decode_override_value, encode_override_value, expand_prefabs,
        expand_prefabs_for_editor,
    },
};
use uuid::Uuid;

fn entity_id(number: u128) -> EntityUuid {
    EntityUuid::try_from_uuid(Uuid::from_u128(number)).unwrap()
}

fn asset_id(number: u128) -> AssetId {
    AssetId::try_from_uuid(Uuid::from_u128(number)).unwrap()
}

fn entity(number: u128, parent: Option<u128>) -> SceneEntity {
    let mut entity = SceneEntity::new(entity_id(number), format!("Entity {number}"));
    entity.parent = parent.map(entity_id);
    entity
}

fn instance(asset: u128) -> PrefabInstance {
    PrefabInstance {
        asset: asset_id(asset),
        overrides: BTreeMap::new(),
        added_components: Default::default(),
        removed_components: Default::default(),
    }
}

fn document(entities: impl IntoIterator<Item = SceneEntity>) -> SceneDocument {
    SceneDocument {
        id: DocumentId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
        assets: BTreeMap::new(),
        entities: entities.into_iter().collect(),
    }
}

fn known(value: ComponentValue) -> SceneComponent {
    SceneComponent::Known {
        version: ComponentSchemaVersion::ONE,
        value,
    }
}

fn raw(input: &str) -> RawRon {
    RawRon::from_ron(input).unwrap()
}

fn kid(namespace: EntityUuid, child: u128) -> EntityUuid {
    EntityUuid::try_from_uuid(Uuid::new_v5(
        &namespace.as_uuid(),
        Uuid::from_u128(child).as_bytes(),
    ))
    .unwrap()
}

fn field(name: &str, shape: impl Into<ComponentValueShape>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
}

fn stats_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("game.Stats"),
        "Stats",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        field("hp", ComponentValueKind::U64),
        field("speed", ComponentValueKind::U64),
        field(
            "pose",
            ComponentValueShape::record([
                ("x".to_owned(), ComponentValueKind::U64.into()),
                ("y".to_owned(), ComponentValueKind::U64.into()),
            ]),
        ),
        field(
            "map",
            ComponentValueShape::map(ComponentValueKind::U64.into()),
        ),
    ])
}

fn stats(hit_points: u64, speed: u64) -> SceneComponent {
    known(ComponentValue::map([
        ("hp", ComponentValue::U64(hit_points)),
        ("speed", ComponentValue::U64(speed)),
        (
            "pose",
            ComponentValue::map([("x", ComponentValue::U64(2)), ("y", ComponentValue::U64(3))]),
        ),
        (
            "map",
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
        ),
    ]))
}

fn value<'a>(entity: &'a SceneEntity, component: &str) -> &'a ComponentValue {
    let record = &entity.components[&ComponentTypeId::new(component)];
    let SceneComponent::Known { value, .. } = record else {
        panic!("expected known value");
    };
    value
}

fn row(expanded: &ExpandedScene, id: EntityUuid) -> &SceneEntity {
    expanded
        .document
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .unwrap()
}

#[test]
fn nested_instances_remap_inherited_references_and_keep_authored_attachments() {
    let reference = || ComponentTypeId::new("game.Links");
    let refs = |targets: &[u128]| {
        known(ComponentValue::map([(
            "links",
            ComponentValue::List(
                targets
                    .iter()
                    .copied()
                    .map(entity_id)
                    .map(ComponentValue::EntityReference)
                    .collect(),
            ),
        )]))
    };
    let mut outer_root = entity(11, None);
    outer_root.components.insert(reference(), refs(&[13, 11]));
    let mut nested_mount = entity(13, Some(11));
    nested_mount.prefab = Some(instance(800));
    let mut attached = entity(14, Some(13));
    attached.components.insert(reference(), refs(&[13]));
    let outer = document([attached, nested_mount, outer_root]);
    let mut inner_root = entity(21, None);
    inner_root
        .components
        .insert(reference(), refs(&[22, 21, 14, 11, 500]));
    let mut inner_child = entity(22, Some(21));
    inner_child
        .components
        .insert(reference(), refs(&[22, 13, 900]));
    let inner = document([inner_child, inner_root]);
    let mut first = entity(100, Some(500));
    first.prefab = Some(instance(700));
    let mut second = entity(200, None);
    second.prefab = Some(instance(700));
    let mut authored_child = entity(101, Some(100));
    authored_child.components.insert(reference(), refs(&[100]));
    let scene = document([second, authored_child, first, entity(500, None)]);
    let mut requested = Vec::new();
    let prefabs = BTreeMap::from([(asset_id(700), outer), (asset_id(800), inner)]);
    let expanded = expand_prefabs(&scene, &ComponentSchemaCatalog::default(), |asset| {
        requested.push(asset);
        prefabs.get(&asset).cloned().ok_or("missing prefab")
    })
    .unwrap();

    assert_eq!(requested, [asset_id(700), asset_id(800)]);
    let mut reordered = scene.clone();
    reordered.entities.reverse();
    let reordered = expand_prefabs(&reordered, &ComponentSchemaCatalog::default(), |asset| {
        let mut source = prefabs.get(&asset).cloned().ok_or("missing prefab")?;
        source.entities.reverse();
        Ok::<_, &str>(source)
    })
    .unwrap();
    assert_eq!(expanded, reordered);
    assert!(
        expanded
            .document
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none())
    );
    assert_eq!(row(&expanded, entity_id(100)).name, "Entity 100");
    assert_eq!(row(&expanded, entity_id(100)).parent, Some(entity_id(500)));
    assert_eq!(row(&expanded, entity_id(101)).parent, Some(entity_id(100)));
    assert!(!expanded.provenance.contains_key(&entity_id(101)));
    let links = |id| match value(row(&expanded, id), "game.Links")
        .get("links")
        .unwrap()
    {
        ComponentValue::List(values) => values
            .iter()
            .map(|reference| {
                let ComponentValue::EntityReference(id) = reference else {
                    panic!("expected entity UUID")
                };
                *id
            })
            .collect::<Vec<_>>(),
        _ => panic!("expected a list of entity UUIDs"),
    };
    for mount in [entity_id(100), entity_id(200)] {
        let nested = kid(mount, 13);
        let child = kid(nested, 22);
        let sibling = kid(mount, 14);
        assert_eq!(links(mount), [nested, mount]);
        assert_eq!(
            links(nested),
            [child, nested, sibling, mount, entity_id(500)]
        );
        assert_eq!(links(child), [child, nested, entity_id(900)]);
        assert_eq!(row(&expanded, child).parent, Some(nested));
        assert_eq!(row(&expanded, sibling).parent, Some(nested));
        assert_eq!(expanded.provenance[&child].instance, mount);
        assert_eq!(
            expanded.provenance[&child].child_path,
            [entity_id(13), entity_id(22)]
        );
        assert_eq!(
            expanded
                .resolve_target(mount, &[entity_id(13), entity_id(22)])
                .unwrap()
                .id,
            child
        );
        assert_eq!(
            expanded.resolve_target(mount, &[entity_id(14)]).unwrap().id,
            sibling
        );
        assert!(
            expanded
                .resolve_target(mount, &[entity_id(13), entity_id(14)])
                .is_none()
        );
        assert_eq!(
            expanded.local_entity_reference(mount, mount),
            Some(entity_id(11))
        );
        assert_eq!(
            expanded.local_entity_reference(mount, nested),
            Some(entity_id(13))
        );
        assert_eq!(
            expanded.local_entity_reference(nested, child),
            Some(entity_id(22))
        );
        assert_eq!(expanded.local_entity_reference(mount, child), None);
        assert_eq!(
            expanded.override_entity_reference(mount, nested),
            Some(entity_id(13))
        );
        assert_eq!(
            expanded.override_entity_reference(mount, child),
            Some(child)
        );
    }
    assert_eq!(
        expanded
            .resolve_target(entity_id(100), &[entity_id(13)])
            .unwrap()
            .id
            .to_string(),
        "06cd034a-25fe-57c1-99d0-bf762c4666f2"
    );
    assert_eq!(
        expanded
            .resolve_target(entity_id(100), &[entity_id(13), entity_id(22)])
            .unwrap()
            .id
            .to_string(),
        "fd4617de-53d5-5ff3-9db1-e6c4c11fe3b2"
    );
}

#[test]
fn nested_sparse_overrides_preserve_unedited_fields_and_revert_to_the_immediate_base() {
    let stats_id = ComponentTypeId::new("game.Stats");
    let mut inner_child = entity(22, Some(21));
    inner_child
        .components
        .insert(stats_id.clone(), stats(10, 75));
    let inner = document([entity(21, None), inner_child]);
    let mut nested = entity(13, Some(11));
    let mut nested_mount = instance(800);
    nested_mount
        .overrides
        .insert(format!("child:{}/game.Stats.hp", entity_id(22)), raw("40"));
    nested.prefab = Some(nested_mount);
    let mut outer_root = entity(11, None);
    let removed = ComponentTypeId::new("game.Old");
    let added = ComponentTypeId::new("game.New");
    outer_root
        .components
        .insert(removed.clone(), known(ComponentValue::Bool(true)));
    let outer = document([outer_root, nested]);
    let catalog = ComponentSchemaCatalog::from_components([stats_schema()]).unwrap();
    let child_path = [entity_id(13), entity_id(22)];
    let hit_points = PrefabOverridePath::new(
        child_path,
        stats_id.clone(),
        ComponentFieldPath::from_fields(["hp"]),
    )
    .unwrap();
    let pose = PrefabOverridePath::new(
        child_path,
        stats_id,
        ComponentFieldPath::from_fields(["pose", "x"]),
    )
    .unwrap();
    let key = hit_points.canonical_key(&catalog);
    let mut mount = instance(700);
    mount.overrides.insert(
        key.clone(),
        encode_override_value(
            &hit_points,
            &ComponentValue::U64(90),
            &catalog,
            &BTreeMap::new(),
        )
        .unwrap(),
    );
    mount
        .overrides
        .insert(pose.canonical_key(&catalog), raw("12"));
    mount.added_components.insert(
        added.clone(),
        known(ComponentValue::String("local".to_owned())),
    );
    mount.removed_components.insert(removed.clone());
    let mut first = entity(100, None);
    first.prefab = Some(mount);
    let mut other = entity(200, None);
    other.prefab = Some(instance(700));
    let mut scene = document([first, other]);
    let load = |asset| match asset {
        id if id == asset_id(700) => Ok(outer.clone()),
        id if id == asset_id(800) => Ok(inner.clone()),
        _ => Err("missing prefab"),
    };

    let expanded = expand_prefabs(&scene, &catalog, load).unwrap();
    assert_eq!(
        expanded
            .resolve_override(entity_id(100), &hit_points)
            .unwrap(),
        ComponentValue::U64(90)
    );
    assert_eq!(
        expanded
            .resolve_override(entity_id(200), &hit_points)
            .unwrap(),
        ComponentValue::U64(40)
    );
    assert_eq!(
        expanded.resolve_override(entity_id(100), &pose).unwrap(),
        ComponentValue::U64(12)
    );
    let target = expanded
        .resolve_target(entity_id(100), &child_path)
        .unwrap();
    assert_eq!(
        value(target, "game.Stats").get("speed"),
        Some(&ComponentValue::U64(75))
    );
    assert_eq!(
        value(target, "game.Stats").get("pose").unwrap().get("y"),
        Some(&ComponentValue::U64(3))
    );
    let root = expanded.resolve_target(entity_id(100), &[]).unwrap();
    assert!(!root.components.contains_key(&removed));
    assert!(root.components.contains_key(&added));
    assert!(
        expanded
            .resolve_target(entity_id(200), &[])
            .unwrap()
            .components
            .contains_key(&removed)
    );
    scene.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .remove(&key);
    let reverted = expand_prefabs(&scene, &catalog, load).unwrap();
    assert_eq!(
        reverted
            .resolve_override(entity_id(100), &hit_points)
            .unwrap(),
        ComponentValue::U64(40)
    );
    assert_eq!(
        reverted.resolve_override(entity_id(100), &pose).unwrap(),
        ComponentValue::U64(12)
    );
}

#[test]
fn override_references_and_aliases_keep_the_declaring_documents_scope() {
    let component = ComponentTypeId::new("game.Binding");
    let schema = ComponentSchema::new(component.clone(), "Binding", ComponentSchemaVersion::ONE)
        .with_fields([
            field("local", ComponentValueShape::EntityRef)
                .with_capability(ComponentCapability::EntityRef),
            field("selected", ComponentValueShape::EntityRef)
                .with_capability(ComponentCapability::EntityRef),
            field(
                "image",
                ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
            )
            .with_capability(ComponentCapability::AssetRef),
        ]);
    let catalog = ComponentSchemaCatalog::from_components([schema]).unwrap();
    let mut root = entity(11, None);
    root.components.insert(
        component,
        known(ComponentValue::map([
            ("local", ComponentValue::EntityReference(entity_id(12))),
            ("selected", ComponentValue::EntityReference(entity_id(13))),
            ("image", ComponentValue::AssetReference(asset_id(20))),
        ])),
    );
    let mut prefab = document([root, entity(12, Some(11)), entity(13, Some(11))]);
    prefab.assets.insert(
        "selected".to_owned(),
        SceneAsset {
            id: asset_id(20),
            hint: AssetPath::new("prefab.png").unwrap(),
        },
    );
    let mut mount = instance(700);
    mount
        .overrides
        .insert("game.Binding.image".to_owned(), raw(r#""@selected""#));
    mount.overrides.insert(
        "game.Binding.selected".to_owned(),
        raw(&format!(r#""{}""#, entity_id(12))),
    );
    let mut mounted = entity(100, None);
    mounted.prefab = Some(mount);
    let mut scene = document([mounted, entity(12, None)]);
    scene.assets.insert(
        "selected".to_owned(),
        SceneAsset {
            id: asset_id(30),
            hint: AssetPath::new("scene.png").unwrap(),
        },
    );
    let load = |_| Ok::<_, &str>(prefab.clone());
    let expanded = expand_prefabs(&scene, &catalog, load).unwrap();
    let bindings = value(row(&expanded, entity_id(100)), "game.Binding");
    assert_eq!(
        bindings.get("local"),
        Some(&ComponentValue::EntityReference(kid(entity_id(100), 12)))
    );
    assert_eq!(
        bindings.get("selected"),
        Some(&ComponentValue::EntityReference(entity_id(12)))
    );
    assert_eq!(
        bindings.get("image"),
        Some(&ComponentValue::AssetReference(asset_id(30)))
    );
    assert_eq!(
        expanded.asset_hints[&asset_id(20)],
        AssetPath::new("prefab.png").unwrap()
    );
    assert_eq!(
        expanded.asset_hints[&asset_id(30)],
        AssetPath::new("scene.png").unwrap()
    );
    assert_eq!(
        expanded.override_entity_reference(entity_id(100), kid(entity_id(100), 12)),
        Some(kid(entity_id(100), 12))
    );
    assert_eq!(
        expanded.override_entity_reference(entity_id(100), kid(entity_id(100), 13)),
        Some(entity_id(13))
    );
}

#[test]
fn editor_defers_only_an_override_on_an_actual_opaque_component() {
    let component = ComponentTypeId::new("game.PowerCell");
    let opaque = SceneComponent::Unknown(UnknownComponent {
        type_id: component.clone(),
        version: ComponentSchemaVersion::ONE,
        raw: raw("(order: 1, future: Mystery(42))"),
    });
    let mut root = entity(11, None);
    let mut child = entity(12, Some(11));
    child.components.insert(component.clone(), opaque.clone());
    root.components
        .insert(ComponentTypeId::new("game.Stats"), stats(10, 75));
    let prefab = document([root, child]);
    let mut mounted = entity(100, None);
    let mut mount = instance(700);
    let address = format!("child:{}/game.PowerCell.order", entity_id(12));
    mount.overrides.insert(address.clone(), raw("2"));
    mounted.prefab = Some(mount);
    let mut scene = document([mounted]);
    let unrelated = ComponentSchema::new(
        ComponentTypeId::new("game"),
        "Game",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field("version", ComponentValueKind::U64)]);
    let catalog = ComponentSchemaCatalog::from_components([stats_schema(), unrelated]).unwrap();
    let load = |_| Ok::<_, &str>(prefab.clone());

    assert!(matches!(expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::Override { source: PrefabOverrideError::OpaqueComponent(id), .. }) if id == component));
    let expanded = expand_prefabs_for_editor(&scene, &catalog, load).unwrap();
    let target = kid(entity_id(100), 12);
    assert_eq!(row(&expanded, target).components[&component], opaque);
    assert_eq!(expanded.deferred_overrides.len(), 1);
    let deferred = &expanded.deferred_overrides[0];
    assert_eq!(deferred.instance, entity_id(100));
    assert_eq!(deferred.target, target);
    assert_eq!(deferred.address, address);
    assert_eq!(deferred.component, component);
    let path = expanded
        .parse_override(entity_id(100), &deferred.address, &catalog)
        .unwrap();
    assert_eq!(path.child_path(), [entity_id(12)]);
    assert_eq!(path.component(), &component);
    assert_eq!(path.field(), &ComponentFieldPath::from_fields(["order"]));
    assert_eq!(
        expanded
            .parse_override(
                entity_id(100),
                &format!("child:{}/game.PowerCell::order", entity_id(12)),
                &catalog,
            )
            .unwrap(),
        path
    );
    assert_eq!(
        scene.entities[0].prefab.as_ref().unwrap().overrides[&address].as_ron(),
        "2"
    );

    let invalid = |document: &mut SceneDocument, address: &str, raw_value: RawRon| {
        document.entities[0]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .insert(address.to_owned(), raw_value);
    };
    let mut wrong_field = scene.clone();
    invalid(&mut wrong_field, "game.Stats.missing", raw("3"));
    assert!(matches!(
        expand_prefabs_for_editor(&wrong_field, &catalog, load),
        Err(PrefabError::Override {
            source: PrefabOverrideError::UnknownField,
            ..
        })
    ));
    let mut wrong_value = scene.clone();
    invalid(&mut wrong_value, "game.Stats.hp", raw("true"));
    assert!(matches!(
        expand_prefabs_for_editor(&wrong_value, &catalog, load),
        Err(PrefabError::Override {
            source: PrefabOverrideError::Value(_),
            ..
        })
    ));
    invalid(
        &mut scene,
        &format!("child:{}/game.PowerCell.order", entity_id(13)),
        raw("3"),
    );
    assert!(matches!(
        expand_prefabs_for_editor(&scene, &catalog, load),
        Err(PrefabError::Override {
            source: PrefabOverrideError::MissingTarget,
            ..
        })
    ));
    assert!(
        matches!(expand_prefabs_for_editor(&scene, &catalog, |_| Err::<SceneDocument, _>("missing")),
        Err(PrefabError::Load { asset, .. }) if asset == asset_id(700))
    );
}

#[test]
fn canonical_addresses_round_trip_with_ambiguous_component_ids_and_quoted_keys() {
    let overlapping = ComponentSchema::new(
        ComponentTypeId::new("game.Stats.pose"),
        "More stats",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field("x", ComponentValueKind::U64)]);
    let catalog = ComponentSchemaCatalog::from_components([stats_schema(), overlapping]).unwrap();
    let ambiguous = PrefabOverridePath::new(
        [],
        ComponentTypeId::new("game.Stats"),
        ComponentFieldPath::from_fields(["pose", "x"]),
    )
    .unwrap();
    assert!(matches!(
        PrefabOverridePath::parse(&ambiguous.to_string(), &catalog),
        Err(PrefabOverrideError::AmbiguousComponent)
    ));
    assert_eq!(ambiguous.canonical_key(&catalog), "game.Stats::pose.x");
    assert_eq!(
        PrefabOverridePath::parse(&ambiguous.canonical_key(&catalog), &catalog).unwrap(),
        ambiguous
    );
    let path = PrefabOverridePath::new(
        [entity_id(13), entity_id(22)],
        ComponentTypeId::new("game.Stats"),
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("map"),
            ComponentFieldPathSegment::field("x.y/a\\\"]::z"),
        ]),
    )
    .unwrap();
    let key = path.canonical_key(&catalog);
    assert!(key.contains("game.Stats.map["));
    assert_eq!(PrefabOverridePath::parse(&key, &catalog).unwrap(), path);
    let aliases = BTreeMap::new();
    let encoded =
        encode_override_value(&path, &ComponentValue::U64(42), &catalog, &aliases).unwrap();
    assert_eq!(encoded.as_ron(), "42");
    assert_eq!(
        decode_override_value(&path, &encoded, &catalog, &aliases).unwrap(),
        ComponentValue::U64(42)
    );
}

#[test]
fn override_addresses_use_the_components_on_the_target() {
    let shorter = ComponentTypeId::new("game.Stats");
    let longer = ComponentTypeId::new("game.Stats.pose");
    let overlap = ComponentSchema::new(longer.clone(), "Pose", ComponentSchemaVersion::ONE)
        .with_fields([field("x", ComponentValueKind::U64)]);
    let catalog = ComponentSchemaCatalog::from_components([stats_schema(), overlap]).unwrap();
    let mut root = entity(11, None);
    root.components.insert(shorter.clone(), stats(10, 75));
    let mut prefab = document([root]);
    let mut mounted = entity(100, None);
    let mut mount = instance(700);
    let dot_key = "game.Stats.pose.x";
    let shorter_key = "game.Stats::pose.x";
    let longer_key = "game.Stats.pose::x";
    mount.overrides.insert(dot_key.to_owned(), raw("12"));
    mounted.prefab = Some(mount);
    let mut scene = document([mounted]);
    let expand = |scene: &SceneDocument, prefab: &SceneDocument| {
        expand_prefabs(scene, &catalog, |_| Ok::<_, &str>(prefab.clone()))
    };

    let expanded = expand(&scene, &prefab).unwrap();
    let path = expanded
        .parse_override(entity_id(100), dot_key, &catalog)
        .unwrap();
    assert_eq!(path.component(), &shorter);
    assert_eq!(
        path.field(),
        &ComponentFieldPath::from_fields(["pose", "x"])
    );
    assert_eq!(
        expanded.resolve_override(entity_id(100), &path).unwrap(),
        ComponentValue::U64(12)
    );
    assert!(matches!(
        expanded.parse_override(entity_id(100), longer_key, &catalog),
        Err(PrefabOverrideError::MissingComponent(component)) if component == longer
    ));

    prefab.entities[0].components.insert(
        longer.clone(),
        known(ComponentValue::map([("x", ComponentValue::U64(3))])),
    );
    assert!(matches!(
        expand(&scene, &prefab),
        Err(PrefabError::Override {
            source: PrefabOverrideError::AmbiguousComponent,
            ..
        })
    ));

    let overrides = &mut scene.entities[0].prefab.as_mut().unwrap().overrides;
    overrides.remove(dot_key);
    overrides.insert(shorter_key.to_owned(), raw("12"));
    overrides.insert(longer_key.to_owned(), raw("34"));
    let expanded = expand(&scene, &prefab).unwrap();
    for (key, component, expected) in [(shorter_key, shorter, 12), (longer_key, longer, 34)] {
        let path = expanded
            .parse_override(entity_id(100), key, &catalog)
            .unwrap();
        assert_eq!(path.component(), &component);
        assert_eq!(
            expanded.resolve_override(entity_id(100), &path).unwrap(),
            ComponentValue::U64(expected)
        );
    }
}

#[test]
fn indexed_packed_overrides_use_the_element_type_and_keep_the_other_cells() {
    let component = ComponentTypeId::new("game.Grid");
    let schema = ComponentSchema::new(component.clone(), "Grid", ComponentSchemaVersion::ONE)
        .with_fields([field("tiles", ComponentValueKind::PackedU16)]);
    let catalog = ComponentSchemaCatalog::from_components([schema]).unwrap();
    let mut root = entity(11, None);
    root.components.insert(
        component,
        known(ComponentValue::map([(
            "tiles",
            ComponentValue::PackedU16(vec![10, 20, 30]),
        )])),
    );
    let prefab = document([root]);
    let load = |_| Ok::<_, &str>(prefab.clone());
    let path = PrefabOverridePath::parse("game.Grid.tiles[1]", &catalog).unwrap();
    let mut mount = instance(700);
    let key = path.canonical_key(&catalog);
    mount.overrides.insert(key.clone(), raw("65535"));
    let mut mounted = entity(100, None);
    mounted.prefab = Some(mount);
    let mut scene = document([mounted]);
    let expanded = expand_prefabs(&scene, &catalog, load).unwrap();
    assert_eq!(
        expanded.resolve_override(entity_id(100), &path).unwrap(),
        ComponentValue::U64(65535)
    );
    assert_eq!(
        value(row(&expanded, entity_id(100)), "game.Grid").get("tiles"),
        Some(&ComponentValue::PackedU16(vec![10, 65535, 30]))
    );
    scene.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .insert(key, raw("65536"));
    assert!(matches!(
        expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::Override {
            source: PrefabOverrideError::Value(_),
            ..
        })
    ));
}

#[test]
fn override_choices_use_the_exact_declared_path() {
    let component = ComponentTypeId::new("game.Config");
    let mode = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("mode"),
        "Mode",
        ComponentFieldPath::from_fields(["settings", "mode"]),
        ComponentValueKind::String,
        ComponentValue::String("safe".into()),
    )
    .with_choices(["safe", "fast"]);
    let schema = ComponentSchema::new(component.clone(), "Config", ComponentSchemaVersion::ONE)
        .with_fields([
            field("count", ComponentValueKind::U64),
            field(
                "labels",
                ComponentValueShape::map(ComponentValueKind::String.into()),
            ),
            mode,
        ]);
    let catalog = ComponentSchemaCatalog::from_components([schema]).unwrap();
    let address = format!("child:{}/game.Config::settings.mode", entity_id(12));
    let path = PrefabOverridePath::parse(&address, &catalog).unwrap();
    let assets = BTreeMap::new();
    let unsupported = r#""custom""#;
    let invalid_choice = |error: &PrefabOverrideError| {
        matches!(
            error,
            PrefabOverrideError::Value(CompactValueError::Shape(ComponentShapeError {
                kind: ComponentShapeErrorKind::UndeclaredChoice(choice),
                ..
            })) if choice == "custom"
        )
    };
    assert!(invalid_choice(
        &encode_override_value(
            &path,
            &ComponentValue::String("custom".into()),
            &catalog,
            &assets,
        )
        .unwrap_err()
    ));
    assert!(invalid_choice(
        &decode_override_value(&path, &raw(unsupported), &catalog, &assets).unwrap_err()
    ));
    for value in [ComponentValue::String("fast".into()), ComponentValue::Null] {
        let raw = encode_override_value(&path, &value, &catalog, &assets).unwrap();
        assert_eq!(
            decode_override_value(&path, &raw, &catalog, &assets).unwrap(),
            value
        );
    }

    let mut child = entity(12, Some(11));
    child.components.insert(
        component,
        known(ComponentValue::map([
            ("count", ComponentValue::U64(0)),
            ("labels", ComponentValue::Map(BTreeMap::new())),
            (
                "settings",
                ComponentValue::map([("mode", ComponentValue::String("safe".into()))]),
            ),
        ])),
    );
    let prefab = document([entity(11, None), child]);
    let load = |_| Ok::<_, &str>(prefab.clone());
    let label_address = format!("child:{}/game.Config.labels.mode", entity_id(12));
    let label = PrefabOverridePath::parse(&label_address, &catalog).unwrap();
    let label_value = encode_override_value(
        &label,
        &ComponentValue::String("custom".into()),
        &catalog,
        &assets,
    )
    .unwrap();
    let mut mount = instance(700);
    mount.overrides = BTreeMap::from([
        (
            format!("child:{}/game.Config.count", entity_id(12)),
            raw("2"),
        ),
        (label_address, label_value),
        (address.clone(), raw(unsupported)),
    ]);
    let mut mounted = entity(100, None);
    mounted.prefab = Some(mount);
    let mut scene = document([mounted]);
    assert!(matches!(
        expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::Override { instance, address: offending, source })
            if instance == entity_id(100) && offending == address && invalid_choice(&source)
    ));
    scene.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .remove(&address);
    let expanded = expand_prefabs(&scene, &catalog, load).unwrap();
    assert_eq!(
        expanded.resolve_override(entity_id(100), &label).unwrap(),
        ComponentValue::String("custom".into())
    );
}

#[test]
fn keyed_list_conflicts_attribute_the_edit_that_changed_a_colliding_key() {
    let component = ComponentTypeId::new("game.Inventory");
    let key = ComponentFieldId::new("stable_key");
    let item = ComponentValueShape::record_members([
        ComponentShapeMember::new("key", ComponentValueKind::String).with_id(key.clone()),
        ComponentShapeMember::new("note", ComponentValueKind::String),
    ]);
    let schema = ComponentSchema::new(component.clone(), "Inventory", ComponentSchemaVersion::ONE)
        .with_fields([
            field("caption", ComponentValueKind::String),
            field(
                "slots",
                ComponentValueShape::optional(ComponentValueShape::keyed_list(item, key)),
            ),
        ]);
    let catalog = ComponentSchemaCatalog::from_components([schema]).unwrap();
    let mut root = entity(11, None);
    root.components.insert(
        component,
        known(ComponentValue::map([
            ("caption", ComponentValue::String("Stored".into())),
            (
                "slots",
                ComponentValue::List(
                    ["A", "B", "C", "D"]
                        .into_iter()
                        .map(|key| {
                            ComponentValue::map([
                                ("key", ComponentValue::String(key.into())),
                                ("note", ComponentValue::String(String::new())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])),
    );
    let prefab = document([root]);
    let load = |_| Ok::<_, &str>(prefab.clone());
    let offender = "game.Inventory.slots[1].key";
    let last = "game.Inventory.slots[3]";
    let mut mount = instance(700);
    mount.overrides = BTreeMap::from([
        ("game.Inventory.caption".to_owned(), raw(r#""Carried""#)),
        ("game.Inventory.slots[0].key".to_owned(), raw(r#""C""#)),
        (offender.to_owned(), raw(r#""D""#)),
        ("game.Inventory.slots[2].key".to_owned(), raw(r#""A""#)),
        (last.to_owned(), raw(r#"(key: "D", note: "edited")"#)),
    ]);
    let mut mounted = entity(100, None);
    mounted.prefab = Some(mount);
    let mut scene = document([mounted]);
    assert!(matches!(
        expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::Override {
            address,
            source: PrefabOverrideError::ComponentValue(_),
            ..
        }) if address == offender
    ));

    scene.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .insert(last.to_owned(), raw(r#"(key: "B", note: "edited")"#));
    let expanded = expand_prefabs(&scene, &catalog, load).unwrap();
    for (index, expected) in ["C", "D", "A", "B"].into_iter().enumerate() {
        let path =
            PrefabOverridePath::parse(&format!("game.Inventory.slots[{index}].key"), &catalog)
                .unwrap();
        assert_eq!(
            expanded.resolve_override(entity_id(100), &path).unwrap(),
            ComponentValue::String(expected.into())
        );
    }
}

#[test]
fn delegated_root_retains_the_outer_mount_and_cannot_override_an_outer_sibling() {
    let stats_id = ComponentTypeId::new("game.Stats");
    let mut child = entity(22, Some(21));
    child.components.insert(stats_id.clone(), stats(10, 75));
    let inner = document([entity(21, None), child]);
    let mut wrapper_root = entity(11, None);
    wrapper_root.prefab = Some(instance(800));
    let mut wrapper_child = entity(10, Some(11));
    wrapper_child.components.insert(stats_id, stats(5, 10));
    let mut wrapper = document([wrapper_root, wrapper_child]);
    let mut mounted = entity(100, None);
    mounted.prefab = Some(instance(700));
    let scene = document([mounted]);
    let catalog = ComponentSchemaCatalog::from_components([stats_schema()]).unwrap();
    let load = |asset| match asset {
        id if id == asset_id(700) => Ok(wrapper.clone()),
        id if id == asset_id(800) => Ok(inner.clone()),
        _ => Err("missing prefab"),
    };
    let expanded = expand_prefabs(&scene, &catalog, load).unwrap();
    assert_eq!(expanded.document.entities.len(), 3);
    assert_eq!(
        expanded.local_entity_reference(entity_id(100), entity_id(100)),
        Some(entity_id(11))
    );
    assert_eq!(
        expanded.local_entity_reference(entity_id(100), kid(entity_id(100), 22)),
        None
    );
    assert_eq!(
        expanded
            .resolve_target(entity_id(100), &[entity_id(22)])
            .unwrap()
            .id,
        kid(entity_id(100), 22)
    );
    wrapper.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .insert(format!("child:{}/game.Stats.hp", entity_id(10)), raw("100"));
    assert!(matches!(
        expand_prefabs(&scene, &catalog, |asset| {
            if asset == asset_id(700) {
                Ok::<_, &str>(wrapper.clone())
            } else {
                Ok(inner.clone())
            }
        }),
        Err(PrefabError::Override {
            source: PrefabOverrideError::MissingTarget,
            ..
        })
    ));
}

#[test]
fn delegated_roots_keep_asset_local_references_across_mounts() {
    let component = ComponentTypeId::new("game.Binding");
    let schema = ComponentSchema::new(component.clone(), "Binding", ComponentSchemaVersion::ONE)
        .with_fields([field("target", ComponentValueShape::EntityRef)
            .with_capability(ComponentCapability::EntityRef)]);
    let catalog = ComponentSchemaCatalog::from_components([schema]).unwrap();
    let address = "game.Binding.target";
    let path = PrefabOverridePath::parse(address, &catalog).unwrap();
    let reference = ComponentValue::EntityReference;
    let encode = |target| {
        encode_override_value(&path, &reference(target), &catalog, &BTreeMap::new()).unwrap()
    };

    let mut inner_root = entity(21, None);
    inner_root.components.insert(
        component,
        known(ComponentValue::map([("target", reference(entity_id(21)))])),
    );
    let inner = document([inner_root, entity(22, Some(21))]);
    let mut wrapper_root = entity(11, None);
    wrapper_root.prefab = Some(instance(800));
    let mut wrapper = document([wrapper_root]);

    let mut first = entity(100, None);
    let mut second = entity(200, None);
    let first_mount = first.id;
    let second_mount = second.id;
    let first_child = kid(first_mount, 22);
    let second_child = kid(second_mount, 22);
    let mut first_prefab = instance(700);
    first_prefab
        .overrides
        .insert(address.to_owned(), encode(first_child));
    first.prefab = Some(first_prefab);
    second.prefab = Some(instance(700));
    let mut scene = document([first, second]);
    let expand = |scene: &SceneDocument, wrapper: &SceneDocument| {
        expand_prefabs(scene, &catalog, |asset| match asset {
            id if id == asset_id(700) => Ok(wrapper.clone()),
            id if id == asset_id(800) => Ok(inner.clone()),
            _ => Err("missing prefab"),
        })
        .unwrap()
    };

    let expanded = expand(&scene, &wrapper);
    for (mount, child) in [(first_mount, first_child), (second_mount, second_child)] {
        assert_eq!(
            expanded.local_entity_reference(mount, mount),
            Some(entity_id(11))
        );
        assert_eq!(expanded.local_entity_reference(mount, child), None);
        assert_eq!(
            expanded.expanded_entity_reference(mount, entity_id(11)),
            Some(mount)
        );
        assert_eq!(
            expanded.expanded_entity_reference(mount, entity_id(21)),
            None
        );
        assert_eq!(
            expanded.local_entity_reference_in(mount, asset_id(700), mount),
            Some(entity_id(11))
        );
        assert_eq!(
            expanded.local_entity_reference_in(mount, asset_id(800), mount),
            Some(entity_id(21))
        );
        assert_eq!(
            expanded.local_entity_reference_in(mount, asset_id(800), child),
            Some(entity_id(22))
        );
        assert_eq!(
            expanded.local_entity_reference_in(mount, asset_id(700), child),
            None
        );
    }
    assert_eq!(
        expanded.local_entity_reference_in(second_mount, asset_id(800), first_child),
        None
    );
    assert_eq!(
        expanded.override_entity_reference(first_mount, first_child),
        Some(first_child)
    );
    let effective = expanded.resolve_override(first_mount, &path).unwrap();
    assert_eq!(effective, reference(first_child));
    assert_eq!(
        expanded.resolve_override(second_mount, &path).unwrap(),
        reference(second_mount)
    );

    let local = expanded
        .local_entity_reference_in(first_mount, asset_id(800), first_child)
        .unwrap();
    wrapper.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .insert(address.to_owned(), encode(local));
    scene.entities[0].prefab.as_mut().unwrap().overrides.clear();
    let applied = expand(&scene, &wrapper);
    assert_eq!(
        applied.resolve_override(first_mount, &path).unwrap(),
        effective
    );
    assert_eq!(
        applied.resolve_override(second_mount, &path).unwrap(),
        reference(second_child)
    );
}

#[test]
fn rejects_override_conflicts_and_generated_id_collisions() {
    let mut root = entity(11, None);
    let stats_id = ComponentTypeId::new("game.Stats");
    root.components.insert(stats_id.clone(), stats(10, 75));
    let prefab = document([root, entity(12, Some(11))]);
    let load = |_| Ok::<_, &str>(prefab.clone());
    let catalog = ComponentSchemaCatalog::from_components([stats_schema()]).unwrap();
    let mut mounted = entity(100, None);
    let mut mount = instance(700);
    mount
        .added_components
        .insert(stats_id.clone(), stats(20, 55));
    mount.removed_components.insert(stats_id.clone());
    mounted.prefab = Some(mount);
    let mut scene = document([mounted]);
    assert!(matches!(expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::AddRemoveConflict { component, .. }) if component == stats_id));
    scene.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .removed_components
        .clear();
    assert!(matches!(expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::DuplicateComponent { component, .. }) if component == stats_id));
    let mount = scene.entities[0].prefab.as_mut().unwrap();
    mount.added_components.clear();
    mount
        .overrides
        .insert("game.Stats.pose".to_owned(), raw("(x: 14, y: 19)"));
    mount
        .overrides
        .insert("game.Stats.pose.x".to_owned(), raw("4"));
    assert!(matches!(
        expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::Override {
            source: PrefabOverrideError::OverlappingField(_),
            ..
        })
    ));
    scene.entities[0].prefab.as_mut().unwrap().overrides.clear();
    let collision = kid(entity_id(100), 12);
    scene.entities.push(SceneEntity::new(collision, "Authored"));
    assert!(matches!(expand_prefabs(&scene, &catalog, load),
        Err(PrefabError::EntityCollision { entity }) if entity == collision));
}

#[test]
fn rejects_bad_roots_cycles_and_expansion_limits() {
    let mut mounted = entity(100, None);
    mounted.prefab = Some(instance(700));
    let scene = document([mounted]);
    let catalog = ComponentSchemaCatalog::default();
    for count in [0, 2] {
        assert!(
            matches!(expand_prefabs(&scene, &catalog, |_| Ok::<_, &str>(document((1..=count).map(|id| entity(id, None))))),
            Err(PrefabError::InvalidRootCount { roots, .. }) if roots == count as usize)
        );
    }
    let chain = |asset: AssetId| {
        let mut root = entity(11, None);
        let number = asset.as_uuid().as_u128();
        root.prefab = Some(instance(if number == 700 { 701 } else { 700 }));
        Ok::<_, &str>(document([root]))
    };
    assert!(matches!(expand_prefabs_for_editor(&scene, &catalog, chain),
        Err(PrefabError::Cycle { chain }) if chain == [asset_id(700), asset_id(701), asset_id(700)]));
    let mut loads = 0;
    let depth_error = expand_prefabs(&scene, &catalog, |asset| {
        loads += 1;
        let mut root = entity(11, None);
        root.prefab = Some(instance(asset.as_uuid().as_u128() + 1));
        Ok::<_, &str>(document([root]))
    });
    assert!(matches!(depth_error,
        Err(PrefabError::DepthLimit { asset, limit }) if limit == MAX_PREFAB_DEPTH && asset == asset_id(700 + limit as u128)));
    assert_eq!(loads, MAX_PREFAB_DEPTH);

    let width = 257;
    let prefab =
        document(std::iter::once(entity(1, None)).chain((2..=width).map(|id| entity(id, Some(1)))));
    let scene = document((1..=width).map(|id| {
        let mut mounted = entity(id + 1000, None);
        mounted.prefab = Some(instance(700));
        mounted
    }));
    assert!(
        matches!(expand_prefabs(&scene, &catalog, |_| Ok::<_, &str>(prefab.clone())),
        Err(PrefabError::EntityLimit { limit }) if limit == MAX_EXPANDED_ENTITIES)
    );
}
