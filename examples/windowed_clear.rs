use nara::{backend_prelude::*, prelude::*};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "nara windowed clear",
                WindowResolution::new(1280, 720),
            )),
        },
        WgpuBackendPlugins,
    ))?;
    app.add_systems(StartupStage::Scene, setup_scene)?;
    WinitRunner::default().install(&mut app)?;

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
