use std::error::Error;

use nara::project_host::{DesktopRun, DesktopRunOptions};
use nara::{backend_prelude::*, prelude::*};

#[path = "support/desktop_run.rs"]
mod desktop_run;

fn main() -> Result<(), Box<dyn Error>> {
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
    let runtime = nara::app::RuntimeInstance::start(app)?;
    desktop_run::finish_desktop_run(DesktopRun::from_runtime(runtime, DesktopRunOptions::new()))?;
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
