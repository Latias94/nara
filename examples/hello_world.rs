use nara::prelude::*;

fn main() -> Result<(), AppRunError> {
    let mut app = App::new();
    app.add_plugins(Runtime2dPlugins)?
        .add_startup_systems(StartupStage::Scene, setup_scene)?
        .add_systems(CoreStage::Update, move_sprites)?;

    app.update()?;
    Ok(())
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d::default(),
    ));

    commands.spawn((
        Name::new("player"),
        Transform2d::from_translation(Vec2::new(0.0, 0.0)),
        Sprite::from_color(Vec2::new(32.0, 32.0), Color::rgb(0.2, 0.7, 1.0)),
        Visibility::Visible,
    ));
}

fn move_sprites(mut transforms: Query<&mut Transform2d>) {
    for mut transform in &mut transforms {
        transform.translation.x += 1.0;
    }
}
