use std::error::Error;

use nara::{backend_prelude::*, prelude::*};

#[path = "support/runtime_retirement.rs"]
mod runtime_retirement;
use runtime_retirement::finish_runtime_after_winit;

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
    let candidate = nara::app::RuntimeCandidate::admit(app.seal()?)?;
    let mut runtime = candidate.complete_startup()?.promote();
    let run_result = WinitRunner::default().run(&mut runtime);
    finish_runtime_after_winit(run_result, runtime)
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
