use std::{env, error::Error};

use game_components::Spin;
use nara::prelude::{
    App, AssetAppExt, Camera2d, Color, DefaultPlugins, Query, RegisterSceneComponent, Res,
    RuntimeAssetSource, Transform2d, Update, VirtualTime,
};

const ENTRY_SCENE: &str = "{entry_scene}";

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "source")]
    let source = RuntimeAssetSource::from_project_dir(env::current_dir()?, "assets")?;
    #[cfg(not(feature = "source"))]
    let source = {
        let path = env::current_exe()?.with_file_name("game.npak");
        RuntimeAssetSource::from_npak(&path)
            .map_err(|error| format!("could not open {}: {error}", path.display()))?
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new(env!("CARGO_PKG_NAME")))?;
    app.set_asset_source(source)?;
    app.register_scene_component::<Spin>()?;
    app.world_mut()?.spawn((
        Camera2d {
            clear_color: Some(Color::rgb(0.04, 0.06, 0.1)),
            ..Camera2d::default()
        },
        Transform2d::default(),
    ));

    let entry = ENTRY_SCENE.parse()?;
    nara::scene::switch_scene_sync(app.world_mut()?, entry)?;

    app.add_systems(Update, spin)?;
    app.run()?;
    Ok(())
}

fn spin(time: Res<VirtualTime>, mut sprites: Query<(&mut Transform2d, &Spin)>) {
    for (mut transform, spin) in &mut sprites {
        transform.rotation += spin.radians_per_second * time.delta_seconds();
    }
}
