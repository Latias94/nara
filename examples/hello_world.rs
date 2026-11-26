use nara::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless())?
        .add_systems(StartupStage::Scene, setup_scene)?
        .add_systems(CoreStage::Update, move_sprites)?;

    let mut runtime = RuntimeInstance::start(app)?;
    runtime.drive(std::time::Duration::ZERO)?;
    runtime.finish()?;
    Ok(())
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Name::new("player"),
        Transform2d::from_xy(0.0, 0.0),
        Sprite::from_color(Vec2::splat(32.0), Color::rgb(0.2, 0.7, 1.0)),
    ));
}

fn move_sprites(mut transforms: Query<&mut Transform2d>) {
    for mut transform in &mut transforms {
        transform.translation.x += 1.0;
    }
}
