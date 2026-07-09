use nara::{
    prelude::*, render::register_render_components, scene::register_scene_components,
    sprite::register_sprite_components, tilemap::register_tilemap_components,
    transform::register_transform_components,
};

fn main() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry);
    register_transform_components(&mut registry);
    register_render_components(&mut registry);
    register_sprite_components(&mut registry);
    register_tilemap_components(&mut registry);

    let catalog = registry.schema_catalog();
    let json = serde_json::to_string_pretty(&catalog).unwrap();

    assert!(json.contains("nara.transform.Transform2d"));
    assert!(json.contains("nara.sprite.Sprite"));
    assert!(json.contains("nara.tilemap.Tilemap"));
    assert!(json.contains("translation"));
    assert!(json.contains("material.image"));
    assert!(!json.contains("bevy_ecs::"));

    println!("{json}");
}
