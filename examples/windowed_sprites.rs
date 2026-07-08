use nara::prelude::*;

fn main() -> Result<(), AppRunError> {
    let mut app = App::new();
    app.add_plugin(MinimalPlugins)?
        .add_plugin(WindowPlugin {
            primary_window: Some(Window::new(
                "nara windowed sprites",
                WindowResolution::new(1280, 720),
            )),
        })?
        .add_plugin(WinitPlugin::default())?
        .add_plugin(WgpuRenderPlugin)?
        .add_startup_systems(StartupStage::Scene, setup_scene);

    app.run()?;
    Ok(())
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            clear_color: Some(Color::rgb(0.05, 0.06, 0.07)),
            viewport_height: 720.0,
            ..Camera2d::default()
        },
    ));

    commands.spawn((
        Name::new("cyan square"),
        Transform2d::from_translation(Vec2::new(-80.0, 20.0)),
        Sprite::from_color(Vec2::new(80.0, 80.0), Color::rgb(0.1, 0.75, 0.95)).with_layer(1),
    ));
    commands.spawn((
        Name::new("rose square"),
        Transform2d {
            translation: Vec2::new(40.0, -20.0),
            rotation: 0.35,
            scale: Vec2::splat(1.0),
        },
        Sprite::from_color(Vec2::new(96.0, 48.0), Color::rgb(0.95, 0.25, 0.35)).with_layer(2),
    ));

    let mut tilemap = Tilemap::new(Vec2::new(32.0, 32.0)).with_layer(0);
    for x in -4..=4 {
        tilemap.set_cell(
            TileCoord::new(x, -3),
            TileCell::new(TileIndex::new(x.unsigned_abs()))
                .with_color(Color::rgb(0.18, 0.36, 0.22)),
        );
    }
    commands.spawn((
        Name::new("floor"),
        Transform2d::from_translation(Vec2::new(0.0, 0.0)),
        tilemap,
    ));
}
