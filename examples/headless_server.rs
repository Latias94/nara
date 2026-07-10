use std::{error::Error, time::Duration};

use nara::prelude::*;

#[derive(Debug, Default, Resource)]
struct ObservedCommands(usize);

fn observe_commands(queue: Res<GameplayCommandQueue>, mut observed: ResMut<ObservedCommands>) {
    observed.0 = queue.as_slice().len();
}

fn main() -> Result<(), Box<dyn Error>> {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Headless Example"

[profiles.server]
"#,
    );

    if load.has_errors() {
        return Err(format!(
            "invalid manifest diagnostics: {:?}",
            load.diagnostics.diagnostics()
        )
        .into());
    }

    let settings = load
        .manifest
        .expect("valid manifest load should carry a manifest")
        .resolve_profile(Some("server"))?;

    let mut app = App::new();
    apply_project_settings(&mut app, settings)?
        .insert_resource(ObservedCommands::default())?
        .add_systems(CoreStage::FixedUpdate, observe_commands)?;

    let command = GameplayCommandEnvelope::new(
        GameplayCommandTypeId::new("server.tick")?,
        GameplayCommandSource::Test,
        GameplayCommandTime {
            frame: 0,
            fixed_tick: Some(0),
        },
    );
    app.world_mut()?
        .resource_mut::<GameplayCommandQueue>()
        .push(command);

    let outcome = app.run_once(Duration::from_secs_f64(1.0 / 60.0))?;

    assert_eq!(outcome.exit, None);
    assert_eq!(app.world().resource::<ObservedCommands>().0, 1);
    assert!(app.world().resource::<GameplayCommandQueue>().is_empty());
    assert!(!app.world().contains_resource::<nara::input::PointerState>());
    assert!(
        !app.world()
            .contains_resource::<nara::window::WindowEvents>()
    );

    Ok(())
}
