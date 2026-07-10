use nara::{
    advanced_prelude::{AssetRecord, AssetSourceKind, ProjectAssetDatabase},
    prelude::*,
    render::register_render_components,
    scene::register_scene_components,
    sprite::register_sprite_components,
    tilemap::register_tilemap_components,
    transform::register_transform_components,
};

const PLAYER_TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const TILESET_ID: &str = "b73f0f16-09e8-4265-b090-b689b41c197e";

fn main() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).expect("scene components should register once");
    register_transform_components(&mut registry)
        .expect("transform components should register once");
    register_render_components(&mut registry).expect("render components should register once");
    register_sprite_components(&mut registry).expect("sprite components should register once");
    register_tilemap_components(&mut registry).expect("tilemap components should register once");

    let scene = sample_scene();
    let asset_database = sample_asset_database();
    let json = scene.to_json_string().unwrap();
    let ron = scene.to_ron_string().unwrap();

    let from_json = SceneDocument::from_json_str(&json).unwrap();
    let from_ron = SceneDocument::from_ron_str(&ron).unwrap();
    assert_eq!(from_json, scene);
    assert_eq!(from_ron, scene);

    let mut invalid = scene.clone();
    invalid.entities[1].components.insert(
        ComponentTypeId::new("nara.transform.Transform2d"),
        SceneComponentRecord::new(
            ComponentSchemaVersion(1),
            ComponentValue::map([
                (
                    "translation",
                    ComponentValue::map([
                        ("x", ComponentValue::String("bad".to_string())),
                        ("y", ComponentValue::f64(0.0).unwrap()),
                    ]),
                ),
                ("rotation", ComponentValue::f64(0.0).unwrap()),
                (
                    "scale",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(1.0).unwrap()),
                        ("y", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
    );
    let invalid_report = invalid.validate_with_asset_database(&registry, &asset_database);
    let diagnostic = invalid_report
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "scene.invalid-component-payload")
        .expect("invalid fixture should produce a repairable component diagnostic");
    assert_eq!(
        diagnostic_identifier(diagnostic, "entity-id"),
        Some("player")
    );
    assert_eq!(
        diagnostic_identifier(diagnostic, "component-id"),
        Some("nara.transform.Transform2d")
    );
    assert_eq!(
        diagnostic_identifier(diagnostic, "field-path"),
        Some("translation.x")
    );

    let mut world = World::new();
    let mut spawner = SceneSpawner::new();
    let spawn_report =
        spawner.spawn_with_asset_database(&mut world, &registry, &from_json, &asset_database);
    assert!(!spawn_report.diagnostics.has_errors());
    assert_eq!(spawn_report.entity_map.len(), 3);

    let export = export_scene(&world, &registry);
    assert!(!export.diagnostics.has_errors());
    let canonical_json = export.document.to_json_string().unwrap();
    assert!(canonical_json.contains("\"path\""));
    assert!(!canonical_json.contains("AssetId"));
    assert_eq!(
        SceneDocument::from_json_str(&canonical_json).unwrap(),
        export.document
    );

    let stable_export = export_scene_with_options(
        &world,
        &registry,
        SceneExportOptions {
            asset_ref_export_policy: AssetRefExportPolicy::StableIdWhenKnown,
        },
    );
    assert!(!stable_export.diagnostics.has_errors());
    let stable_json = stable_export.document.to_json_string().unwrap();
    assert!(stable_json.contains("\"stable_id\""));
    assert!(stable_json.contains(PLAYER_TEXTURE_ID));
    assert!(stable_json.contains(TILESET_ID));
    assert!(!stable_json.contains("AssetId"));
}

fn diagnostic_identifier<'a>(diagnostic: &'a Diagnostic, key: &str) -> Option<&'a str> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .and_then(|field| match field.value() {
            nara::diagnostic::DiagnosticValueRef::Identifier(value) => Some(value),
            _ => None,
        })
}

fn sample_scene() -> SceneDocument {
    SceneDocument::new([
        SceneEntityRecord::new(scene_id("camera"))
            .with_component(
                name_id(),
                SceneComponentRecord::new(v1(), name_value("camera")),
            )
            .with_component(
                transform_id(),
                SceneComponentRecord::new(v1(), transform_value(0.0, 0.0)),
            )
            .with_component(camera_id(), SceneComponentRecord::new(v1(), camera_value())),
        SceneEntityRecord::new(scene_id("player"))
            .with_component(
                name_id(),
                SceneComponentRecord::new(v1(), name_value("player")),
            )
            .with_component(
                transform_id(),
                SceneComponentRecord::new(v1(), transform_value(4.0, 2.0)),
            )
            .with_component(sprite_id(), SceneComponentRecord::new(v1(), sprite_value())),
        SceneEntityRecord::new(scene_id("tiles"))
            .with_component(
                name_id(),
                SceneComponentRecord::new(v1(), name_value("tiles")),
            )
            .with_component(
                transform_id(),
                SceneComponentRecord::new(v1(), transform_value(0.0, -2.0)),
            )
            .with_component(
                tilemap_id(),
                SceneComponentRecord::new(v1(), tilemap_value()),
            ),
    ])
}

fn name_value(name: &str) -> ComponentValue {
    ComponentValue::String(name.to_string())
}

fn transform_value(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("translation", vec2_value(x, y)),
        ("rotation", ComponentValue::f64(0.0).unwrap()),
        ("scale", vec2_value(1.0, 1.0)),
    ])
}

fn camera_value() -> ComponentValue {
    ComponentValue::map([
        (
            "target",
            ComponentValue::String("primary_window".to_string()),
        ),
        ("viewport", ComponentValue::Null),
        ("clear_color", ComponentValue::Null),
        ("viewport_height", ComponentValue::f64(720.0).unwrap()),
        ("order", ComponentValue::I64(0)),
    ])
}

fn sprite_value() -> ComponentValue {
    ComponentValue::map([
        ("size", vec2_value(32.0, 32.0)),
        (
            "material",
            ComponentValue::map([
                ("image", asset_ref_value("stable_id", PLAYER_TEXTURE_ID)),
                ("sampler", sampler_value()),
                ("alpha_mode", ComponentValue::String("blend".to_string())),
                ("tint", color_value(0.2, 0.7, 1.0, 1.0)),
            ]),
        ),
        ("layer", ComponentValue::I64(1)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn tilemap_value() -> ComponentValue {
    ComponentValue::map([
        ("tile_size", vec2_value(16.0, 16.0)),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("tileset", asset_ref_value("stable_id", TILESET_ID)),
        (
            "cells",
            ComponentValue::List(vec![ComponentValue::map([
                (
                    "coord",
                    ComponentValue::map([
                        ("x", ComponentValue::I64(0)),
                        ("y", ComponentValue::I64(0)),
                    ]),
                ),
                (
                    "cell",
                    ComponentValue::map([
                        ("tile", ComponentValue::U64(1)),
                        ("color", color_value(0.2, 0.9, 0.4, 1.0)),
                    ]),
                ),
            ])]),
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

fn asset_ref_value(kind: &str, value: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_string())),
        ("value", ComponentValue::String(value.to_string())),
    ])
}

fn sample_asset_database() -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            StableAssetId::parse_str(PLAYER_TEXTURE_ID).unwrap(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    database
        .insert(AssetRecord::new(
            StableAssetId::parse_str(TILESET_ID).unwrap(),
            AssetPath::new("tilesets/basic.tileset.ron").unwrap(),
            AssetSourceKind::Other("tileset".to_string()),
        ))
        .unwrap();
    database
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn v1() -> ComponentSchemaVersion {
    ComponentSchemaVersion(1)
}

fn name_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.scene.Name")
}

fn transform_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.transform.Transform2d")
}

fn camera_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.render.Camera2d")
}

fn sprite_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.sprite.Sprite")
}

fn tilemap_id() -> ComponentTypeId {
    ComponentTypeId::new("nara.tilemap.Tilemap")
}
