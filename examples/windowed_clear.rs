use std::error::Error;

use nara::{prelude::*, render_wgpu::WgpuRenderObservation, window::WindowId};

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new("Nara windowed clear"))?
        .add_systems(Startup, setup_scene)?;
    if std::env::args_os().any(|argument| argument == "--verify-first-present") {
        app.add_systems(Last, exit_after_present)?;
        if app.run()? != AppExit::Success {
            return Err("the window closed before its first presentation".into());
        }
        println!("First window frame presented");
    } else {
        app.run()?;
    }
    Ok(())
}

fn exit_after_present(observation: Res<WgpuRenderObservation>, mut exit: ResMut<AppExitRequests>) {
    if observation
        .configured_surface_extent(WindowId::PRIMARY)
        .is_some()
    {
        exit.request_success();
    }
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
