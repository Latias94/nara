use nara::{
    prelude::*, scene::register_scene_components, sprite::register_sprite_components,
    transform::register_transform_components,
};

fn main() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry);
    register_transform_components(&mut registry);
    register_sprite_components(&mut registry);

    let player = scene_id("player");
    let enemy = scene_id("enemy");
    let mut scene = SceneDocument::new([
        SceneEntityRecord::new(player.clone()).with_component(
            sprite_id(),
            SceneComponentRecord::new(v1(), sprite_value(ComponentValue::Null)),
        ),
        SceneEntityRecord::new(enemy.clone()),
    ]);
    let original = scene.clone();
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: player.clone(),
            component: transform_id(),
            value: SceneComponentRecord::new(v1(), transform_value(4.0, 2.0)),
        },
        ScenePatchOperation::SetAssetRefField {
            entity: player.clone(),
            component: sprite_id(),
            component_version: v1(),
            path: ComponentFieldPath::from_fields(["texture"]),
            asset_ref: AssetRef::path("textures/player.png").unwrap(),
        },
        ScenePatchOperation::Reparent {
            entity: enemy.clone(),
            parent: Some(player.clone()),
        },
    ]);

    let json = serde_json::to_string_pretty(&patch).unwrap();
    let ron = ron::ser::to_string_pretty(&patch, ron::ser::PrettyConfig::default()).unwrap();
    let from_json = serde_json::from_str::<ScenePatchDocument>(&json).unwrap();
    let from_ron = ron::from_str::<ScenePatchDocument>(&ron).unwrap();
    assert_eq!(from_json, patch);
    assert_eq!(from_ron, patch);
    assert_no_runtime_ids(&json);
    assert_no_runtime_ids(&ron);

    let report = from_json.apply_to_scene(&mut scene, &registry);
    assert!(report.applied);
    assert!(!report.diagnostics.has_errors());
    assert_eq!(
        scene
            .entities
            .iter()
            .find(|entity| entity.id == enemy)
            .unwrap()
            .parent
            .as_ref(),
        Some(&player)
    );

    let inverse = report.inverse.unwrap();
    let undo = inverse.apply_to_scene(&mut scene, &registry);
    assert!(undo.applied);
    assert_eq!(scene, original);

    println!("{json}");
}

fn assert_no_runtime_ids(serialized: &str) {
    for forbidden in ["AssetId", "Handle", "Entity", "wgpu"] {
        assert!(!serialized.contains(forbidden));
    }
}

fn transform_value(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("translation", vec2_value(x, y)),
        ("rotation", ComponentValue::f64(0.0).unwrap()),
        ("scale", vec2_value(1.0, 1.0)),
    ])
}

fn sprite_value(texture: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        ("size", vec2_value(32.0, 32.0)),
        ("color", color_value(1.0, 1.0, 1.0, 1.0)),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("texture", texture),
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

fn transform_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.transform.Transform2d")
}

fn sprite_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.sprite.Sprite")
}
