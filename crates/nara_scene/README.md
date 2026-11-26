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

Enable `nara_scene/authoring` to work with `.nara.scene.ron` and `.nara.prefab.ron`. Both use a
flat `SceneDocument` with a UUID, entity rows, and versioned component records. Each row has its
own UUID, a name, and an optional parent UUID in the same document. A component value uses the
tagged Reflect wire format. Unknown component IDs and versions retain their values across saves.

```rust
use nara_scene::{authoring::{SceneDocument, SceneEntity}, spatial::Transform2d};

fn save() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = SceneDocument::new("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f".parse()?);
    let mut lamp = SceneEntity::new("916da482-3796-4722-a414-98a0cf318a28".parse()?, "Lamp");
    lamp.set(&Transform2d::from_xy(10.0, 2.0))?;
    document.entities.push(lamp);
    document.write("relay.nara.scene.ron")?;
    Ok(())
}
```

Use `SceneEntity::typed::<T>()` to read values via a `PersistentComponentProvider`. The built-in
schemas for transforms, visibility, sprites, and colliders come with creation values from
`authoring::builtin_component_schemas()`. Combine these with the game's component catalog for an
inspector. `SpriteSource::image_asset_path()` validates the optional project-relative PNG path.

`SceneDocument::parse` and `to_ron` work in memory. Native `read` and `write` use UTF-8 files.
Saving validates the document and atomically replaces the file in its existing parent directory.
The resulting RON uses UUID order for the entity table and key order within component maps.
