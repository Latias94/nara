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
