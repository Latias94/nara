use std::{
    error::Error,
    time::{Duration, Instant},
};

use nara::{prelude::*, render_wgpu::desktop::DesktopPresentation, window::WindowId};

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::new("Nara windowed clear"))?
        .add_systems(Startup, setup_scene)?;
    if std::env::args_os().any(|argument| argument == "--verify-first-present") {
        let deadline = Instant::now() + Duration::from_secs(15);
        app.add_systems(
            Last,
            move |observation: Res<DesktopPresentation>, mut exit: ResMut<AppExitRequests>| {
                if observation
                    .latest()
                    .is_some_and(|frame| frame.window == WindowId::PRIMARY)
                {
                    exit.request_success();
                } else if Instant::now() >= deadline {
                    exit.request_exit();
                }
            },
        )?;
        if app.run()? != AppExit::Success {
            return Err("the window closed or timed out before its first presentation".into());
        }
        println!("First window frame presented");
    } else {
        app.run()?;
    }
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
