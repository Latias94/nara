use nara::advanced_prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(Runtime2dPlugins)?
        .add_systems(StartupStage::Scene, setup_scene)?;

    app.update()?;

    let batches = app.world().resource::<SpriteBatches>();
    assert_eq!(batches.total_instances(), 3);
    Ok(())
}

fn setup_scene(mut commands: Commands) {
    let target = Handle::<RenderImage2d>::new(AssetId::from_raw(1));
    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            target: RenderTarget::Image(target),
            viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
            viewport_height: 180.0,
            clear_color: Some(Color::rgb(0.06, 0.07, 0.08)),
            ..Camera2d::default()
        },
    ));

    commands.spawn((
        Name::new("marker"),
        Transform2d::from_translation(Vec2::new(-24.0, 0.0)),
        Sprite::from_color(Vec2::new(24.0, 24.0), Color::rgb(0.2, 0.7, 1.0)).with_layer(1),
    ));

    let mut tilemap = Tilemap::new(Vec2::new(16.0, 16.0)).with_layer(0);
    tilemap.set_cell(
        TileCoord::new(0, 0),
        TileCell::new(TileIndex::new(1)).with_color(Color::rgb(0.2, 0.9, 0.4)),
    );
    tilemap.set_cell(
        TileCoord::new(1, 0),
        TileCell::new(TileIndex::new(2)).with_color(Color::rgb(0.9, 0.8, 0.2)),
    );
    commands.spawn((Name::new("ground"), Transform2d::default(), tilemap));
}
