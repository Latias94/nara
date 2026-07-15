use nara::prelude::*;

#[derive(Component)]
struct Player;

fn movement_system(_players: Query<&mut Transform2d>) {}

fn main() -> Result<(), AddPluginsError> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)?;
    app.world_mut()?.spawn(Player);
    let _ = movement_system;
    Ok(())
}
