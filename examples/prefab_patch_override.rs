use nara::{
    core::SourceUniverseId,
    identity::EntityLookup,
    prelude::*,
    scene::{
        InMemoryPrefabSourceResolver, PrefabInstance, SourceDocumentKey, SourceDocumentKind,
        register_scene_components,
    },
    sprite::register_sprite_components,
};

fn main() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).expect("scene components should register once");
    register_sprite_components(&mut registry).expect("sprite components should register once");
    registry
        .freeze()
        .expect("component registry should freeze before scene expansion");

    let source = AssetRef::path("prefabs/enemy.ron").unwrap();
    let universe: SourceUniverseId = "f037335a-e03a-41c7-8fb1-310ff21dd9be".parse().unwrap();
    let enemy_source =
        SourceDocumentKey::named(universe, "example.enemy", SourceDocumentKind::Prefab).unwrap();
    let enemy_prefab =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("visual"))
            .with_component(sprite_id(), SceneComponentRecord::new(v1(), sprite_value()))]);
    let resolver =
        InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), enemy_source, enemy_prefab);
    let overrides = ScenePatchDocument::new([
        ScenePatchOperation::SetField {
            entity: scene_id("visual"),
            component: sprite_id(),
            component_version: v1(),
            field: ComponentFieldId::new("material.tint.r"),
            value: ComponentValue::f64(0.25).unwrap(),
        },
        ScenePatchOperation::SetAssetRefField {
            entity: scene_id("visual"),
            component: sprite_id(),
            component_version: v1(),
            field: ComponentFieldId::new("material.image"),
            asset_ref: AssetRef::path("textures/enemy.png").unwrap(),
        },
    ]);
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord {
        id: scene_id("enemy"),
        children: Vec::new(),
        components: Default::default(),
        prefab: Some(PrefabInstance { source, overrides }),
    }]);

    let expansion = scene.expand_prefabs(&registry, &resolver);
    assert!(!expansion.diagnostics.has_errors());
    let candidate = expansion
        .candidate
        .unwrap()
        .bind_source(scene_source_key("main/source"));
    let expanded = candidate.document();
    assert_eq!(expanded.entities.len(), 2);
    let visual_id = candidate
        .provenance()
        .iter()
        .find_map(|(id, source)| {
            (source.local_entity_id() == &scene_id("visual") && source.projection_path().is_some())
                .then_some(id)
        })
        .expect("the expanded visual retains its source identity");
    let json = expanded.to_json_string().unwrap();
    assert!(json.contains("textures/enemy.png"));
    assert!(!json.contains("AssetId"));

    let mut world = World::new();
    let report = nara::scene::spawn_scene(
        &mut world,
        &registry,
        &candidate,
        &scene_occurrence("main/report"),
        None,
    );
    assert!(!report.diagnostics.has_errors());
    let visual = match report
        .instance
        .as_ref()
        .expect("successful scene spawn should publish an instance")
        .resolve(&world, visual_id)
    {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected resolved prefab entity, got {lookup:?}"),
    };
    let sprite = world.get::<Sprite>(visual).unwrap();
    assert_eq!(sprite.material.tint.r, 0.25);
    assert!(sprite.material.image.is_some());

    println!("{json}");
}

fn sprite_value() -> ComponentValue {
    ComponentValue::map([
        ("size", vec2_value(32.0, 32.0)),
        (
            "material",
            ComponentValue::map([
                ("image", ComponentValue::Null),
                ("sampler", sampler_value()),
                ("alpha_mode", ComponentValue::String("blend".to_string())),
                ("tint", color_value(1.0, 1.0, 1.0, 1.0)),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn sampler_value() -> ComponentValue {
    ComponentValue::map([
        ("min_filter", ComponentValue::String("linear".to_string())),
        ("mag_filter", ComponentValue::String("linear".to_string())),
        (
            "mipmap_filter",
            ComponentValue::String("linear".to_string()),
        ),
        (
            "address_mode_u",
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
        (
            "address_mode_v",
            ComponentValue::String("clamp_to_edge".to_string()),
        ),
    ])
}

fn vec2_value(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(x).unwrap()),
        ("y", ComponentValue::f64(y).unwrap()),
    ])
}

fn color_value(r: f64, g: f64, b: f64, a: f64) -> ComponentValue {
    ComponentValue::map([
        ("r", ComponentValue::f64(r).unwrap()),
        ("g", ComponentValue::f64(g).unwrap()),
        ("b", ComponentValue::f64(b).unwrap()),
        ("a", ComponentValue::f64(a).unwrap()),
    ])
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn v1() -> ComponentSchemaVersion {
    ComponentSchemaVersion(1)
}

fn sprite_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.sprite.Sprite")
}

fn scene_source_key(name: &str) -> nara::scene::SourceDocumentKey {
    nara::scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara::scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara::scene::SceneOccurrenceKey {
    nara::scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
