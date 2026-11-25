use std::error::Error;

use game_components::Spin;
use nara::{
    prelude::{
        App, Camera2d, Color, Commands, DefaultPlugins, Query, RegisterSchema, Res, Sprite,
        Startup, Update, Vec2, VirtualTime,
    },
    scene::spatial::Transform2d,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(env!("CARGO_PKG_NAME")))?;
    app.register_schema::<Spin>()?;
    app.add_systems(Startup, setup)?.add_systems(Update, spin)?;
    app.run()?;
    Ok(())
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d {
            clear_color: Some(Color::rgb(0.04, 0.06, 0.1)),
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));
    commands.spawn((
        Sprite::from_color(Vec2::new(160.0, 96.0), Color::rgb(0.22, 0.72, 0.9)),
        Transform2d::default(),
        Spin {
            radians_per_second: 0.8,
        },
    ));
}

fn spin(time: Res<VirtualTime>, mut sprites: Query<(&mut Transform2d, &Spin)>) {
    for (mut transform, spin) in &mut sprites {
        transform.rotation += spin.radians_per_second * time.delta_seconds();
    }
}
