# `nara_scene`

Create scene entities with ordinary Bevy ECS components. Use `ChildOf` to attach an entity to a
parent, `Name` to label it, and `Transform2d` and `Visibility` to control its local spatial state.

```rust
use std::time::Duration;

use nara_app::App;
use nara_scene::{
    Name, SpatialPlugin,
    spatial::{ChildOf, GlobalTransform2d, Transform2d},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugin(SpatialPlugin)?;
    let world = app.world_mut()?;
    let parent = world.spawn((Name::new("Room"), Transform2d::from_xy(10.0, 0.0))).id();
    let child = world.spawn((Transform2d::from_xy(2.0, 0.0), ChildOf(parent))).id();

    app.update_with_delta(Duration::ZERO)?;
    let global = app.world().get::<GlobalTransform2d>(child).unwrap();
    assert_eq!(global.translation().x, 12.0);
    Ok(())
}
```

`SpatialPlugin` propagates global transforms and inherited visibility at startup, around fixed
simulation, and in `PostUpdate`. Systems in the same schedule can order themselves against
`SpatialSet`. If you own the Bevy schedules directly, call `spatial::register_spatial` once per
world and schedule `spatial::propagate_spatial` after local writes and deferred hierarchy commands.

## Source documents

Enable `nara_scene/authoring` to work with `.scene.ron` and `.prefab.ron`. A version 1 document
holds a document UUID, an optional asset alias table, and a flat entity table. Each row has its own
UUID, a name, and an optional parent UUID from the same document. Known component payloads use
plain RON fields with schema defaults omitted. Vectors and colors use coordinate tuples, while
packed fields use plain typed arrays. Non-default component schema versions are stored in
`component_versions` or `added_component_versions` beside their payload maps.

```rust
use nara_scene::{authoring::{SceneDocument, SceneEntity}, spatial::Transform2d};

fn save() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = SceneDocument::new("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f".parse()?);
    let mut lamp = SceneEntity::new("916da482-3796-4722-a414-98a0cf318a28".parse()?, "Lamp");
    lamp.set(&Transform2d::from_xy(10.0, 2.0))?;
    document.entities.push(lamp);
    document.write("relay.scene.ron")?;
    Ok(())
}
```

Use `SceneEntity::typed::<T>()` to read values through `Schema`. The built-in schemas cover
transforms, visibility, sprites, and colliders. To read and save known game components, pass the
game's schemas to `parse_with`, `read_with`, `to_ron_with`, or `write_with`. An unregistered ID or
unavailable schema version retains its original RON payload, including enum names and comments.
`resolve_unknown_components` can decode these records after a schema refresh and reports records
that remain opaque because decoding failed.

An asset alias combines a stable UUID with a portable path hint relative to the asset root. A
reference such as `"@lamp_texture"` resolves to the alias UUID. A reference can also use a UUID
string directly. Prefab rows use `prefab` for that reference and may also have `overrides`,
`added_components`, and `removed_components` fields.

`SceneDocument::parse` and `to_ron` work in memory. Native `read` and `write` use UTF-8 files.
Saving validates the document and atomically replaces the file in its existing parent directory.
The writer orders entities by UUID, aliases and component IDs lexicographically, and known fields
by their schema declaration order. Native writes use `Prefab(...)` for `.prefab.ron` paths and
`Scene(...)` for scene paths.
