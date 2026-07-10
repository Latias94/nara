use nara::{backend_prelude::*, prelude::*};

fn main() -> Result<(), AppRunError> {
    let mut app = App::new();
    app.add_plugins(Runtime2dPlugins)?
        .add_plugin(WindowPlugin {
            primary_window: Some(Window::new(
                "nara windowed clear",
                WindowResolution::new(1280, 720),
            )),
        })?
        .add_plugin(WinitPlugin::default())?
        .add_plugin(WgpuRenderPlugin)?
        .add_startup_systems(StartupStage::Scene, setup_scene)?;

    app.run()?;
    Ok(())
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            clear_color: Some(Color::rgb(0.08, 0.1, 0.13)),
            ..Camera2d::default()
        },
    ));
}
