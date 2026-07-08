use nara::prelude::*;

#[test]
fn sprite_stable_asset_id_is_rejected_before_world_mutation() {
    let mut registry = ComponentRegistry::new();
    nara::scene::register_scene_components(&mut registry);
    nara::sprite::register_sprite_components(&mut registry);

    let document =
        SceneDocument::new([
            SceneEntityRecord::new(SceneEntityId::new("player").unwrap())
                .with_component(
                    ComponentTypeId::new("nara.scene.Name"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion(1),
                        ComponentValue::String("Player".to_string()),
                    ),
                )
                .with_component(
                    ComponentTypeId::new("nara.sprite.Sprite"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion(1),
                        sprite_with_stable_id_texture(),
                    ),
                ),
        ]);
    let mut world = World::new();
    let before = world.iter_entities().count();

    let report = spawn_scene(&mut world, &registry, &document);

    assert!(report.diagnostics.has_errors());
    assert_eq!(world.iter_entities().count(), before);
    assert!(report.entity_map.is_empty());
    assert!(report.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "scene.invalid-component-payload"
            && diagnostic.context.entity_id.as_deref() == Some("player")
            && diagnostic.context.component_id.as_deref() == Some("nara.sprite.Sprite")
            && diagnostic.context.field_path.as_deref() == Some("texture.value")
            && diagnostic.context.asset_ref.as_deref() == Some("asset-123")
    }));
}

fn sprite_with_stable_id_texture() -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "color",
            ComponentValue::map([
                ("r", ComponentValue::f64(1.0).unwrap()),
                ("g", ComponentValue::f64(1.0).unwrap()),
                ("b", ComponentValue::f64(1.0).unwrap()),
                ("a", ComponentValue::f64(1.0).unwrap()),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        (
            "texture",
            ComponentValue::map([
                ("kind", ComponentValue::String("stable_id".to_string())),
                ("value", ComponentValue::String("asset-123".to_string())),
            ]),
        ),
    ])
}
